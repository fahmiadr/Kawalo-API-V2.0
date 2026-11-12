//! `GET /attendance`: attendance log of the caller's active group; check-in/out with face
//! verification, its nonce and photo, manual requests for when the face fails, and the summary
//! for reports.

mod check;
mod check_repository;
mod entity;
mod manual;
mod manual_repository;
mod repository;
mod rules;
mod stats;

use axum::Router;
use axum::extract::State;
use axum::routing::{get, patch, post};

use crate::core::extract::{CurrentUser, ValidatedQuery};
use crate::core::response::{ApiResult, ok};
use crate::core::state::AppState;
use crate::features::period_query::PeriodQuery;

use entity::AttendanceItem;

const MSG_LISTED: &str = "Daftar data absensi berhasil diambil";

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/attendance", get(list).post(check::check))
        .route("/attendance/nonce", post(check::nonce))
        .route("/attendance/stats", get(stats::stats))
        .route("/attendance/{id}/photo", get(check::photo))
        .route(
            "/attendance/manual-requests",
            get(manual::list).post(manual::create),
        )
        .route("/attendance/manual-requests/{id}", patch(manual::decide))
}

async fn list(
    ValidatedQuery(query): ValidatedQuery<PeriodQuery>,
    user: CurrentUser,
    State(state): State<AppState>,
) -> ApiResult<Vec<AttendanceItem>> {
    let Some(group_uid) = user.group_uid.as_deref().filter(|g| !g.is_empty()) else {
        return ok(Vec::new(), MSG_LISTED);
    };
    let items = repository::list(&state.db, &query.into_filter(group_uid)).await?;
    ok(items, MSG_LISTED)
}
