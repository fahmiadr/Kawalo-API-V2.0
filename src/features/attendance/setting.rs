//! `GET`/`PUT /attendance/settings`: the minimum face similarity a check-in/out must reach, per
//! group. No row means the `FACE_MATCH_THRESHOLD` default.

use axum::extract::State;
use serde::{Deserialize, Serialize};

use super::check::percent;
use super::setting_repository as repository;
use crate::core::clean::iso_string;
use crate::core::error::ApiError;
use crate::core::extract::schema::{Body, Field, Kind, Schema};
use crate::core::extract::{AuthedJson, CurrentUser};
use crate::core::response::{ApiResult, ok};
use crate::core::state::AppState;
use crate::features::faces::unprocessable;

pub const MIN_PERCENT: i32 = 20;
pub const MAX_PERCENT: i32 = 95;

#[derive(Debug, Serialize)]
pub struct Setting {
    face_match_percent: i32,
    default_percent: i32,
    min_percent: i32,
    max_percent: i32,
    is_default: bool,
    updated_by: Option<String>,
    updated_at: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SettingBody {
    face_match_percent: Option<f64>,
}

impl Body for SettingBody {
    // Range and integrality are checked in the handler, so the 422 can name the range in Indonesian.
    const SCHEMA: Schema = Schema::new(&[Field::nullable(
        "face_match_percent",
        Kind::number(f64::MIN, f64::MAX),
    )]);
}

/// Group's configured threshold as a cosine score; `None` when the group uses the default.
pub async fn group_threshold(state: &AppState, group_uid: &str) -> Result<Option<f32>, ApiError> {
    let row = repository::find(&state.db, group_uid).await?;
    Ok(row.map(|r| r.percent as f32 / 100.0))
}

async fn current(state: &AppState, group_uid: &str) -> Result<Setting, ApiError> {
    let default_percent = percent(state.config.faces.match_threshold);
    let row = repository::find(&state.db, group_uid).await?;
    Ok(Setting {
        face_match_percent: row.as_ref().map_or(default_percent, |r| r.percent),
        default_percent,
        min_percent: MIN_PERCENT,
        max_percent: MAX_PERCENT,
        is_default: row.is_none(),
        updated_by: row.as_ref().map(|r| r.updated_by.clone()),
        updated_at: row.map(|r| iso_string(r.updated_at)),
    })
}

pub async fn get(user: CurrentUser, State(state): State<AppState>) -> ApiResult<Setting> {
    let group_uid = user.require_group_uid()?;
    ok(
        current(&state, group_uid).await?,
        "Pengaturan absensi berhasil diambil",
    )
}

pub async fn put(
    State(state): State<AppState>,
    AuthedJson(auth, body): AuthedJson<SettingBody>,
) -> ApiResult<Setting> {
    // Validated before the role check, like every body-carrying route.
    let wanted = body
        .face_match_percent
        .map(|value| {
            let whole = value.fract() == 0.0 && (f64::from(MIN_PERCENT)..=f64::from(MAX_PERCENT)).contains(&value);
            if whole {
                Ok(value as i32)
            } else {
                Err(unprocessable(format!(
                    "face_match_percent harus bilangan bulat {MIN_PERCENT} sampai {MAX_PERCENT}, atau null untuk memakai nilai bawaan."
                )))
            }
        })
        .transpose()?;
    let user = auth.require_role(&["ADMIN", "SUPER_ADMIN"])?;
    let group_uid = user.require_group_uid()?.to_owned();

    let before = current(&state, &group_uid).await?;
    match wanted {
        Some(percent) => {
            repository::upsert(&state.db, &group_uid, percent, &user.email).await?;
        }
        None => repository::delete(&state.db, &group_uid).await?,
    }
    let after = current(&state, &group_uid).await?;
    tracing::info!(
        group = %group_uid,
        old = before.face_match_percent,
        new = after.face_match_percent,
        actor = %user.email,
        "[Attendance] face match percent changed"
    );
    ok(after, "Pengaturan absensi berhasil disimpan")
}
