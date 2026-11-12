//! `POST /schedules/generate/preview` and `POST /schedules/generate` (forecast), ADMIN and
//! SUPER_ADMIN of the token's group only.
//!
//! The commit never receives a plan from the client. It reloads shifts, members and rows inside
//! its transaction, runs the generator again with the same parameters and seed, and accepts only
//! when the recomputed `preview_token` equals the one the preview returned. The token is an
//! HMAC-SHA256 over the normalised parameters, the seed, the ids to delete and the plan, keyed by
//! the active RS256 private key, so a client cannot mint one for a plan it never previewed and
//! any change in the data since the preview turns into a 409. A key rotation between preview and
//! commit also gives 409 (the client previews again).

use aws_lc_rs::encoding::AsDer;
use aws_lc_rs::{constant_time, hmac};
use axum::Router;
use axum::extract::State;
use axum::routing::post;
use chrono::{Days, Local, NaiveDate};
use serde::Serialize;
use sqlx::{Connection, MySqlConnection};

use super::generate_dto::{CommitBody, CommitResponse, GenerateBody, MAX_SEED, PreviewResponse};
use super::generate_repository::{self as repo, NewRow};
use super::generator::{
    self, FillMode, Input, Person, Plan, PlanItem, RestDays, ShiftSpec, check_range,
    default_selection,
};
use super::manage_repository::{conflict, lock_group, release_group};
use crate::core::error::ApiError;
use crate::core::extract::AuthedJson;
use crate::core::response::{ApiResult, created, ok};
use crate::core::state::AppState;

const ROLES: &[&str] = &["ADMIN", "SUPER_ADMIN"];
const TOKEN_DOMAIN: &[u8] = b"kawalo.schedule.generate.v1\0";

const MSG_PREVIEW: &str = "Pratinjau jadwal otomatis dibuat";
const MSG_STALE: &str = "Data jadwal berubah sejak pratinjau dibuat. Buat pratinjau ulang.";
const MSG_CLASH: &str = "Jadwal bentrok dengan data yang baru disimpan. Buat pratinjau ulang.";
const MSG_CONFIRM: &str =
    "Mode timpa menghapus jadwal yang ada. Kirim konfirmasi (confirm_overwrite) untuk melanjutkan.";
const MSG_NOTHING: &str = "Tidak ada jadwal untuk disimpan.";
const MSG_NO_GUARD: &str =
    "Tidak ada personil GUARD aktif di grup ini. Pilih personil secara manual.";

pub fn generate_router() -> Router<AppState> {
    Router::new()
        .route("/schedules/generate/preview", post(preview))
        .route("/schedules/generate", post(commit))
}

fn bad_request(message: impl Into<String>) -> ApiError {
    ApiError::validation().with_message(message)
}

struct Prepared {
    plan: Plan,
    token: hmac::Tag,
}

/// Everything the token binds, serialised in a fixed field order.
#[derive(Serialize)]
struct TokenInput<'a> {
    group: &'a str,
    start: String,
    end: String,
    shifts: Vec<(&'a str, u32)>,
    personnel: Vec<&'a str>,
    fill_mode: FillMode,
    rest: RestDays,
    seed: u32,
    delete_ids: &'a [i64],
    plan: &'a [PlanItem],
}

fn token_key(state: &AppState) -> Result<hmac::Key, ApiError> {
    let signing = state.keys.signing_key().map_err(ApiError::internal)?;
    let der = signing.key.as_der().map_err(ApiError::internal)?;
    Ok(hmac::Key::new(hmac::HMAC_SHA256, der.as_ref()))
}

async fn resolve_shifts(
    conn: &mut MySqlConnection,
    group: &str,
    body: &GenerateBody,
) -> Result<Vec<ShiftSpec>, ApiError> {
    let uids: Vec<&str> = body.shifts.iter().map(|s| s.shift_uid.trim()).collect();
    let found = repo::shifts(conn, group, &uids).await?;
    let mut missing = Vec::new();
    let mut specs = Vec::new();
    for (q, uid) in body.shifts.iter().zip(&uids) {
        match found.iter().find(|s| s.uid.eq_ignore_ascii_case(uid)) {
            Some(s) => specs.push(ShiftSpec {
                quota: q.quota,
                ..s.clone()
            }),
            None => missing.push(*uid),
        }
    }
    if missing.is_empty() {
        Ok(specs)
    } else {
        Err(bad_request(format!(
            "Shift tidak ditemukan di grup ini: {}",
            missing.join(", ")
        )))
    }
}

async fn resolve_personnel(
    conn: &mut MySqlConnection,
    group: &str,
    body: &GenerateBody,
) -> Result<Vec<Person>, ApiError> {
    let members = repo::members(conn, group).await?;
    let person = |m: &repo::Member| Person {
        email: m.email.clone(),
        name: m.name.clone(),
        avatar_url: m.avatar_url.clone(),
    };
    let Some(requested) = body.personnel() else {
        let guards: Vec<Person> = default_selection(&members, |m| &m.role)
            .into_iter()
            .map(person)
            .collect();
        return if guards.is_empty() {
            Err(bad_request(MSG_NO_GUARD))
        } else {
            Ok(guards)
        };
    };
    let mut missing = Vec::new();
    let mut people = Vec::new();
    for email in &requested {
        match members.iter().find(|m| m.email.eq_ignore_ascii_case(email)) {
            Some(m) => people.push(person(m)),
            None => missing.push(email.as_str()),
        }
    }
    if missing.is_empty() {
        Ok(people)
    } else {
        Err(bad_request(format!(
            "Personil bukan anggota aktif grup ini: {}",
            missing.join(", ")
        )))
    }
}

/// Loads the data, runs the generator and signs the result. Same code for preview and commit,
/// which is what makes the token comparison meaningful.
async fn prepare(
    conn: &mut MySqlConnection,
    key: &hmac::Key,
    group: &str,
    body: &GenerateBody,
    seed: u32,
    today: NaiveDate,
) -> Result<Prepared, ApiError> {
    let (start, end) = body
        .dates()
        .ok_or_else(|| ApiError::invalid_input("start_date/end_date"))?;
    check_range(start, end, today).map_err(bad_request)?;
    let shifts = resolve_shifts(conn, group, body).await?;
    let personnel = resolve_personnel(conn, group, body).await?;
    let rest = body.rest();
    // Enough history for the streak cap; one day covers the Malam -> Pagi rule.
    let lookback = if rest.enabled {
        rest.max_consecutive.max(1)
    } else {
        1
    };
    let from = start
        .checked_sub_days(Days::new(u64::from(lookback)))
        .unwrap_or(start);
    let existing = repo::existing(conn, group, from, end, today).await?;
    let plan = generator::generate(&Input {
        start,
        end,
        shifts: &shifts,
        personnel: &personnel,
        existing: &existing,
        fill_mode: body.fill_mode,
        rest,
        seed,
    });
    let input = TokenInput {
        group,
        start: start.to_string(),
        end: end.to_string(),
        shifts: shifts.iter().map(|s| (s.uid.as_str(), s.quota)).collect(),
        personnel: personnel.iter().map(|p| p.email.as_str()).collect(),
        fill_mode: body.fill_mode,
        rest,
        seed,
        delete_ids: &plan.delete_ids,
        plan: &plan.items,
    };
    let mut ctx = hmac::Context::with_key(key);
    ctx.update(TOKEN_DOMAIN);
    ctx.update(&serde_json::to_vec(&input).map_err(ApiError::internal)?);
    let token = ctx.sign();
    Ok(Prepared { plan, token })
}

/// Does not write; the seed is drawn when absent and always returned.
pub async fn preview(
    State(state): State<AppState>,
    AuthedJson(auth, body): AuthedJson<GenerateBody>,
) -> ApiResult<PreviewResponse> {
    let user = auth.require_role(ROLES)?;
    let group = user.require_group_uid()?;
    let seed = body
        .seed
        .unwrap_or_else(|| rand::random::<u32>() & MAX_SEED);
    let key = token_key(&state)?;
    let mut conn = state.db.acquire().await?;
    let today = Local::now().date_naive();
    let Prepared { plan, token } = prepare(&mut conn, &key, group, &body, seed, today).await?;
    ok(
        PreviewResponse {
            seed,
            fill_mode: body.fill_mode,
            preview_token: hex::encode(token.as_ref()),
            existing_count: plan.existing_count,
            delete_count: u32::try_from(plan.delete_ids.len()).unwrap_or(u32::MAX),
            locked_count: plan.locked_count,
            plan: plan.items,
            stats: plan.stats,
        },
        MSG_PREVIEW,
    )
}

/// One transaction under the group lock; any failure rolls everything back.
pub async fn commit(
    State(state): State<AppState>,
    AuthedJson(auth, body): AuthedJson<CommitBody>,
) -> ApiResult<CommitResponse> {
    let user = auth.require_role(ROLES)?;
    let group = user.require_group_uid()?;
    if body.params.fill_mode == FillMode::Overwrite && !body.confirm_overwrite {
        return Err(bad_request(MSG_CONFIRM));
    }
    let key = token_key(&state)?;
    let mut conn = lock_group(&state.db, group).await?;
    let result = commit_locked(&mut conn, &key, group, &body).await;
    release_group(conn, group).await;
    let (created_count, deleted) = result?;
    created(
        CommitResponse {
            created: created_count,
            deleted,
        },
        format!("Jadwal otomatis tersimpan. Dibuat: {created_count}, Dihapus: {deleted}."),
    )
}

async fn commit_locked(
    conn: &mut MySqlConnection,
    key: &hmac::Key,
    group: &str,
    body: &CommitBody,
) -> Result<(u64, u64), ApiError> {
    let params = &body.params;
    let seed = params.seed.ok_or_else(|| ApiError::invalid_input("seed"))?;
    let today = Local::now().date_naive();
    let mut tx = conn.begin().await?;
    let Prepared { plan, token } = prepare(&mut tx, key, group, params, seed, today).await?;
    let sent = hex::decode(body.preview_token.trim()).unwrap_or_default();
    if constant_time::verify_slices_are_equal(token.as_ref(), &sent).is_err() {
        return Err(conflict(MSG_STALE));
    }
    if plan.items.is_empty() {
        return Err(bad_request(MSG_NOTHING));
    }
    let rows: Vec<NewRow<'_>> = plan
        .items
        .iter()
        .filter_map(|i| {
            Some(NewRow {
                date: NaiveDate::parse_from_str(&i.date, "%Y-%m-%d").ok()?,
                shift_uid: &i.shift_uid,
                email: &i.email,
                name: &i.name,
            })
        })
        .collect();
    let written = async {
        let deleted = repo::delete(&mut tx, group, &plan.delete_ids).await?;
        let created = repo::insert(&mut tx, group, &rows).await?;
        Ok::<_, sqlx::Error>((created, deleted))
    }
    .await;
    let counts = match written {
        Ok(counts) => counts,
        Err(sqlx::Error::Database(e)) if e.is_unique_violation() => {
            return Err(conflict(MSG_CLASH));
        }
        Err(e) => return Err(e.into()),
    };
    tx.commit().await?;
    Ok(counts)
}
