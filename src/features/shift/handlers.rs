//! Handlers of `/shifts`. Every member reads; only the `ADMIN` and `SUPER_ADMIN` roles write
//! (the role claim, not the `allow_admin` flag, which some guards carry).

use axum::extract::rejection::PathRejection;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use chrono::{Datelike, Local, Utc};

use super::dto::{ListQuery, ShiftBody, ShiftItem};
use super::repository;
use crate::core::clean::clean_string;
use crate::core::error::{ApiError, ErrorCode};
use crate::core::extract::{AuthContext, AuthedJson, CurrentUser, ValidatedQuery};
use crate::core::response::{ApiResult, created, ok};
use crate::core::state::AppState;

const MANAGER_ROLES: [&str; 2] = ["ADMIN", "SUPER_ADMIN"];
const UID_MAX: usize = 64;
const MSG_NOT_FOUND: &str = "Shift tidak ditemukan.";

fn not_found() -> ApiError {
    ApiError::not_found().with_message(MSG_NOT_FOUND)
}

fn conflict(message: impl Into<String>) -> ApiError {
    ApiError::new(StatusCode::CONFLICT, ErrorCode::ConflictError, message)
}

/// Elysia validates params before authentication, so this runs first in every `{uid}` handler.
fn path_uid(uid: Result<Path<String>, PathRejection>) -> Result<String, ApiError> {
    let Path(uid) = uid.map_err(|rejection| ApiError::invalid_input(rejection.body_text()))?;
    let valid = (1..=UID_MAX).contains(&uid.len())
        && uid
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
    if valid {
        Ok(uid)
    } else {
        Err(ApiError::invalid_input(
            "/uid: Expected string to match '^[A-Za-z0-9_-]{1,64}$'",
        ))
    }
}

pub async fn list(
    ValidatedQuery(query): ValidatedQuery<ListQuery>,
    user: CurrentUser,
    State(state): State<AppState>,
) -> ApiResult<Vec<ShiftItem>> {
    const MESSAGE: &str = "Daftar shift berhasil diambil";
    let Some(group_uid) = user.group_uid.as_deref().filter(|g| !g.is_empty()) else {
        return ok(Vec::new(), MESSAGE);
    };
    let search = clean_string(query.search.as_deref());
    ok(
        repository::list(&state.db, group_uid, search.as_deref()).await?,
        MESSAGE,
    )
}

pub async fn create(
    State(state): State<AppState>,
    AuthedJson(auth, body): AuthedJson<ShiftBody>,
) -> ApiResult<ShiftItem> {
    let user = auth.require_role(&MANAGER_ROLES)?;
    let group_uid = user.require_group_uid()?;
    let shift = body.into_input()?;
    if repository::name_taken(&state.db, group_uid, &shift.name, None).await? {
        return Err(conflict("Nama shift sudah dipakai di grup ini."));
    }
    // 32 hex characters: `tkschedule.SchedShiftUID` and `tkabsen.AShiftUID` are VARCHAR(32).
    let uid = uuid::Uuid::now_v7().simple().to_string();
    repository::insert(&state.db, group_uid, &uid, &shift).await?;
    let item = repository::find(&state.db, group_uid, &uid)
        .await?
        .ok_or_else(|| ApiError::internal("Gagal memetakan shift yang baru dibuat"))?;
    created(item, "Shift berhasil dibuat")
}

pub async fn update(
    State(state): State<AppState>,
    uid: Result<Path<String>, PathRejection>,
    AuthedJson(auth, body): AuthedJson<ShiftBody>,
) -> ApiResult<ShiftItem> {
    let uid = path_uid(uid)?;
    let user = auth.require_role(&MANAGER_ROLES)?;
    let group_uid = user.require_group_uid()?;
    let shift = body.into_input()?;
    repository::find(&state.db, group_uid, &uid)
        .await?
        .ok_or_else(not_found)?;
    if repository::name_taken(&state.db, group_uid, &shift.name, Some(&uid)).await? {
        return Err(conflict("Nama shift sudah dipakai di grup ini."));
    }
    repository::replace(&state.db, group_uid, &uid, &shift).await?;
    let item = repository::find(&state.db, group_uid, &uid)
        .await?
        .ok_or_else(|| ApiError::internal("Gagal memetakan shift yang diperbarui"))?;
    ok(item, "Shift berhasil diperbarui")
}

pub async fn delete_one(
    State(state): State<AppState>,
    uid: Result<Path<String>, PathRejection>,
    AuthContext(auth): AuthContext,
) -> ApiResult<()> {
    let uid = path_uid(uid)?;
    let user = auth.require_role(&MANAGER_ROLES)?;
    let group_uid = user.require_group_uid()?;
    repository::find(&state.db, group_uid, &uid)
        .await?
        .ok_or_else(not_found)?;

    // "Today" follows the process time zone, like `GET /schedules/me`.
    let today = Local::now().date_naive();
    let today_key =
        today.year() * 10_000 + i32::try_from(today.month() * 100 + today.day()).unwrap_or(0);
    let used = repository::count_schedules_from(&state.db, group_uid, &uid, today_key).await?;
    if used > 0 {
        return Err(conflict(format!(
            "Shift masih dipakai {used} jadwal mulai hari ini. Hapus atau pindahkan jadwal tersebut lebih dulu."
        )));
    }
    repository::soft_delete(&state.db, group_uid, &uid, Utc::now().naive_utc()).await?;
    ok((), "Shift berhasil dihapus")
}
