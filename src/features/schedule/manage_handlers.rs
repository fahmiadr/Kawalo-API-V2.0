//! Admin management of the group schedule. Writers are serialised per group with a named DB lock
//! and run in a transaction.

use std::collections::HashSet;

use axum::extract::rejection::PathRejection;
use axum::extract::{Path, State};
use chrono::{Days, Local, NaiveDate};
use serde_json::json;
use sqlx::{Connection, MySqlConnection};

use super::entity::ManagedItem;
use super::manage_dto::{
    BulkBody, BulkResult, DeleteBody, DeleteResult, Entry, ListQuery, ScheduleBody, SkippedItem,
};
use super::manage_plan::{Candidate, Plan, Skip, plan};
use super::manage_repository::{self as repo, Filter, MSG_EXISTS, NewRow, conflict};
use crate::core::error::{ApiError, ErrorCode};
use crate::core::extract::{
    AuthContext, AuthedJson, CurrentUser, ValidatedQuery, ValidationErrors,
};
use crate::core::response::{ApiResult, created, ok};
use crate::core::state::AppState;

/// Only the category in the active group counts, not the `allow_admin` flag.
const MANAGER_ROLES: [&str; 2] = ["ADMIN", "SUPER_ADMIN"];
const MAX_AHEAD_DAYS: u64 = 366;

const MSG_NOT_MEMBER: &str = "Personil bukan anggota aktif grup ini.";
const MSG_BAD_SHIFT: &str = "Shift tidak ditemukan di grup ini.";
const MSG_PAST: &str = "Jadwal untuk tanggal yang sudah lewat tidak bisa dibuat atau diubah.";
const MSG_TOO_FAR: &str = "Jadwal maksimal 366 hari ke depan.";
const MSG_OTHER_SHIFT: &str = "Personil sudah punya jadwal shift lain pada tanggal ini.";
const MSG_LOCKED: &str = "Jadwal yang sudah lewat atau sudah diabsen tidak bisa diubah.";
const MSG_NOT_FOUND: &str = "Jadwal tidak ditemukan.";

fn manager(user: &CurrentUser) -> Result<&str, ApiError> {
    user.require_role(&MANAGER_ROLES)?;
    user.require_group_uid()
}

fn today() -> NaiveDate {
    Local::now().date_naive()
}

fn bad_request(message: &str) -> ApiError {
    ApiError::validation().with_message(message)
}

/// Elysia validates params before auth, so this runs first in every handler with `{id}`.
fn path_id(id: Result<Path<String>, PathRejection>) -> Result<i32, ApiError> {
    let id = id
        .map(|Path(id)| id)
        .map_err(|rejection| ApiError::invalid_input(rejection.body_text()))?;
    let mut errors = ValidationErrors::new();
    errors.digits("id", &id);
    errors
        .into_result()
        .map_err(|errors| ApiError::invalid_input(errors.to_string()))?;
    id.parse::<i32>()
        .ok()
        .filter(|&n| n > 0)
        .ok_or_else(|| ApiError::not_found().with_message(MSG_NOT_FOUND))
}

fn skip_error(skip: Skip) -> ApiError {
    match skip {
        Skip::Exists => conflict(MSG_EXISTS),
        Skip::OtherShift => conflict(MSG_OTHER_SHIFT).with_code(ErrorCode::ScheduleOtherShift),
        Skip::Locked => conflict(MSG_LOCKED),
    }
}

fn skip_code(skip: Skip) -> Option<ErrorCode> {
    (skip == Skip::OtherShift).then_some(ErrorCode::ScheduleOtherShift)
}

fn skip_reason(skip: Skip) -> &'static str {
    match skip {
        Skip::Exists => "Sudah ada",
        Skip::OtherShift => "Sudah punya shift lain pada tanggal ini",
        Skip::Locked => "Jadwal yang sudah diabsen tidak bisa ditimpa",
    }
}

struct Prepared {
    candidates: Vec<Candidate>,
    /// Canonical `(email, name)` per candidate, from the group membership.
    people: Vec<(String, String)>,
}

/// Checks date window, membership and shift of every entry. A failure rejects the whole request;
/// `indexed` prefixes the message with the item index and lists every offender in `details`.
async fn prepare(
    conn: &mut MySqlConnection,
    group: &str,
    entries: &[Entry],
    today: NaiveDate,
    indexed: bool,
) -> Result<Prepared, ApiError> {
    let mut emails: Vec<String> = Vec::new();
    let mut uids: Vec<String> = Vec::new();
    let mut seen = HashSet::new();
    for e in entries {
        if seen.insert(e.email.trim().to_lowercase()) {
            emails.push(e.email.trim().to_owned());
        }
    }
    let mut seen = HashSet::new();
    for e in entries {
        if seen.insert(e.shift_uid.as_str()) {
            uids.push(e.shift_uid.clone());
        }
    }
    let members = repo::members(conn, group, &emails).await?;
    let shifts = repo::valid_shifts(conn, group, &uids).await?;
    let limit = today
        .checked_add_days(Days::new(MAX_AHEAD_DAYS))
        .unwrap_or(today);

    let mut problems: Vec<(usize, &str)> = Vec::new();
    let mut candidates = Vec::with_capacity(entries.len());
    let mut people = Vec::with_capacity(entries.len());
    for (index, e) in entries.iter().enumerate() {
        let key = e.email.trim().to_lowercase();
        let problem = if e.date < today {
            Some(MSG_PAST)
        } else if e.date > limit {
            Some(MSG_TOO_FAR)
        } else if !members.contains_key(&key) {
            Some(MSG_NOT_MEMBER)
        } else if !shifts.contains(&e.shift_uid) {
            Some(MSG_BAD_SHIFT)
        } else {
            None
        };
        if let Some(problem) = problem {
            problems.push((index, problem));
            continue;
        }
        if let Some(person) = members.get(&key) {
            people.push(person.clone());
        }
        candidates.push(Candidate {
            email_key: key,
            shift_uid: e.shift_uid.clone(),
            date: e.date,
        });
    }
    let Some(&(first, reason)) = problems.first() else {
        return Ok(Prepared { candidates, people });
    };
    if !indexed {
        return Err(bad_request(reason));
    }
    let details: Vec<_> = problems
        .iter()
        .take(50)
        .map(|(index, reason)| json!({ "index": index, "reason": reason }))
        .collect();
    Err(bad_request(&format!("Item {first}: {reason}")).with_details(json!(details)))
}

fn date_span(candidates: &[Candidate]) -> Option<(NaiveDate, NaiveDate)> {
    let min = candidates.iter().map(|c| c.date).min()?;
    let max = candidates.iter().map(|c| c.date).max()?;
    Some((min, max))
}

async fn plan_for(
    conn: &mut MySqlConnection,
    group: &str,
    prepared: &Prepared,
    overwrite: bool,
) -> Result<Plan, ApiError> {
    let Some((from, to)) = date_span(&prepared.candidates) else {
        return Ok(Plan::default());
    };
    let emails: Vec<String> = prepared.people.iter().map(|p| p.0.clone()).collect();
    let mut occupancy = repo::occupancy(conn, group, &emails, from, to).await?;
    Ok(plan(&prepared.candidates, &mut occupancy, overwrite))
}

/// Deletes the replaced rows and inserts the planned ones; returns the last inserted id.
async fn apply(
    conn: &mut MySqlConnection,
    group: &str,
    entries: &[Entry],
    prepared: &Prepared,
    plan: &Plan,
) -> Result<u64, ApiError> {
    repo::delete_ids(conn, group, &plan.delete).await?;
    let rows: Vec<NewRow<'_>> = plan
        .insert
        .iter()
        .map(|&i| NewRow {
            shift_uid: &entries[i].shift_uid,
            email: &prepared.people[i].0,
            name: &prepared.people[i].1,
            date: entries[i].date,
        })
        .collect();
    repo::insert_many(conn, group, &rows).await
}

pub async fn list(
    ValidatedQuery(query): ValidatedQuery<ListQuery>,
    user: CurrentUser,
    State(state): State<AppState>,
) -> ApiResult<Vec<ManagedItem>> {
    let group = manager(&user)?;
    let today = today();
    let range = query
        .resolve(today)
        .map_err(|errors| ApiError::invalid_input(errors.to_string()))?;
    let non_empty = |v: &Option<String>| -> Option<String> {
        v.as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
    };
    let (email, shift_uid) = (non_empty(&query.email), non_empty(&query.shift_uid));
    let filter = Filter {
        range: Some(range),
        email: email.as_deref(),
        shift_uid: shift_uid.as_deref(),
        ..Filter::default()
    };
    let items = repo::items(&state.db, group, today, &filter).await?;
    ok(items, "Jadwal berhasil diambil")
}

async fn create_work(
    conn: &mut MySqlConnection,
    group: &str,
    entry: &Entry,
    overwrite: bool,
) -> Result<u64, ApiError> {
    let mut tx = conn.begin().await?;
    let entries = std::slice::from_ref(entry);
    let prepared = prepare(&mut tx, group, entries, today(), false).await?;
    let plan = plan_for(&mut tx, group, &prepared, overwrite).await?;
    if let Some(&(_, skip)) = plan.skipped.first() {
        return Err(skip_error(skip));
    }
    let id = apply(&mut tx, group, entries, &prepared, &plan).await?;
    tx.commit().await?;
    Ok(id)
}

pub async fn create(
    State(state): State<AppState>,
    AuthedJson(auth, body): AuthedJson<ScheduleBody>,
) -> ApiResult<ManagedItem> {
    let user = auth.require_role(&MANAGER_ROLES)?;
    let group = user.require_group_uid()?;
    let mut conn = repo::lock_group(&state.db, group).await?;
    let result = create_work(&mut conn, group, &body.entry, body.overwrite).await;
    repo::release_group(conn, group).await;
    let id = i32::try_from(result?).map_err(|_| ApiError::internal("Id jadwal di luar batas"))?;
    one(&state, group, id)
        .await
        .and_then(|item| created(item, "Jadwal berhasil dibuat"))
}

async fn one(state: &AppState, group: &str, id: i32) -> Result<ManagedItem, ApiError> {
    let filter = Filter {
        id: Some(id),
        ..Filter::default()
    };
    repo::items(&state.db, group, today(), &filter)
        .await?
        .pop()
        .ok_or_else(|| ApiError::internal("Gagal memuat jadwal yang baru disimpan"))
}

async fn bulk_work(
    conn: &mut MySqlConnection,
    group: &str,
    body: &BulkBody,
) -> Result<BulkResult, ApiError> {
    let mut tx = conn.begin().await?;
    let prepared = prepare(&mut tx, group, &body.items, today(), true).await?;
    let plan = plan_for(&mut tx, group, &prepared, body.overwrite).await?;
    apply(&mut tx, group, &body.items, &prepared, &plan).await?;
    tx.commit().await?;
    Ok(BulkResult {
        created: plan.insert.len(),
        replaced: plan.delete.len(),
        skipped: plan.skipped.len(),
        skipped_items: plan
            .skipped
            .iter()
            .map(|&(index, skip)| SkippedItem {
                index,
                reason: skip_reason(skip),
                code: skip_code(skip),
            })
            .collect(),
    })
}

pub async fn bulk(
    State(state): State<AppState>,
    AuthedJson(auth, body): AuthedJson<BulkBody>,
) -> ApiResult<BulkResult> {
    let user = auth.require_role(&MANAGER_ROLES)?;
    let group = user.require_group_uid()?;
    let mut conn = repo::lock_group(&state.db, group).await?;
    let result = bulk_work(&mut conn, group, &body).await;
    repo::release_group(conn, group).await;
    let result = result?;
    created(result, "Jadwal massal berhasil diproses")
}

async fn update_work(
    conn: &mut MySqlConnection,
    group: &str,
    id: i32,
    entry: &Entry,
) -> Result<(), ApiError> {
    let today = today();
    let mut tx = conn.begin().await?;
    let current = repo::stored(&mut tx, group, &[id], today)
        .await?
        .pop()
        .ok_or_else(|| ApiError::not_found().with_message(MSG_NOT_FOUND))?;
    if current.locked {
        return Err(conflict(MSG_LOCKED));
    }
    let entries = std::slice::from_ref(entry);
    let prepared = prepare(&mut tx, group, entries, today, false).await?;
    let (Some(candidate), Some(person)) = (prepared.candidates.first(), prepared.people.first())
    else {
        return Err(ApiError::internal("Hasil validasi jadwal kosong"));
    };
    if current.email.to_lowercase() == candidate.email_key
        && current.shift_uid == candidate.shift_uid
        && current.date == candidate.date
    {
        return Ok(());
    }
    let mut occupancy = repo::occupancy(
        &mut tx,
        group,
        std::slice::from_ref(&person.0),
        entry.date,
        entry.date,
    )
    .await?;
    for rows in occupancy.values_mut() {
        rows.retain(|e| e.id != Some(id));
    }
    let plan = plan(&prepared.candidates, &mut occupancy, false);
    if let Some(&(_, skip)) = plan.skipped.first() {
        return Err(skip_error(skip));
    }
    let row = NewRow {
        shift_uid: &entry.shift_uid,
        email: &person.0,
        name: &person.1,
        date: entry.date,
    };
    repo::update_row(&mut tx, group, id, &row).await?;
    tx.commit().await?;
    Ok(())
}

/// `overwrite` in the body is ignored: moving a row never replaces another one.
pub async fn update(
    State(state): State<AppState>,
    id: Result<Path<String>, PathRejection>,
    AuthedJson(auth, body): AuthedJson<ScheduleBody>,
) -> ApiResult<ManagedItem> {
    let id = path_id(id)?;
    let user = auth.require_role(&MANAGER_ROLES)?;
    let group = user.require_group_uid()?;
    let mut conn = repo::lock_group(&state.db, group).await?;
    let result = update_work(&mut conn, group, id, &body.entry).await;
    repo::release_group(conn, group).await;
    result?;
    one(&state, group, id)
        .await
        .and_then(|item| ok(item, "Jadwal berhasil diperbarui"))
}

pub async fn delete_one(
    State(state): State<AppState>,
    id: Result<Path<String>, PathRejection>,
    AuthContext(auth): AuthContext,
) -> ApiResult<()> {
    let id = path_id(id)?;
    let user = auth.require_role(&MANAGER_ROLES)?;
    let group = user.require_group_uid()?;
    let today = today();
    let mut conn = state.db.acquire().await?;
    let current = repo::stored(&mut conn, group, &[id], today)
        .await?
        .pop()
        .ok_or_else(|| ApiError::not_found().with_message(MSG_NOT_FOUND))?;
    if current.locked || repo::delete_unlocked(&mut conn, group, &[id], today).await? == 0 {
        return Err(conflict(MSG_LOCKED));
    }
    ok((), "Jadwal berhasil dihapus")
}

/// Locked rows and ids that do not exist in the group are skipped; 404 only when none exist.
pub async fn delete_batch(
    State(state): State<AppState>,
    AuthedJson(auth, body): AuthedJson<DeleteBody>,
) -> ApiResult<DeleteResult> {
    let user = auth.require_role(&MANAGER_ROLES)?;
    let group = user.require_group_uid()?;
    let today = today();
    let mut seen = HashSet::new();
    let ids: Vec<i32> = body
        .ids
        .iter()
        .filter_map(|id| id.parse::<i32>().ok())
        .filter(|&id| id > 0 && seen.insert(id))
        .collect();
    let requested =
        u64::try_from(body.ids.iter().collect::<HashSet<_>>().len()).unwrap_or(u64::MAX);
    let mut conn = state.db.acquire().await?;
    let found = repo::stored(&mut conn, group, &ids, today).await?;
    if found.is_empty() {
        return Err(ApiError::not_found().with_message(MSG_NOT_FOUND));
    }
    let deletable: Vec<i32> = found.iter().filter(|s| !s.locked).map(|s| s.id).collect();
    let deleted_count = repo::delete_unlocked(&mut conn, group, &deletable, today).await?;
    ok(
        DeleteResult {
            deleted_count,
            skipped_count: requested.saturating_sub(deleted_count),
        },
        format!("{deleted_count} jadwal berhasil dihapus"),
    )
}
