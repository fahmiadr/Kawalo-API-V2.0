use axum::extract::State;
use chrono::Local;

use super::dto::ScheduleQuery;
use super::entity::ScheduleItem;
use super::repository;
use crate::core::error::ApiError;
use crate::core::extract::{CurrentUser, ValidatedQuery};
use crate::core::response::{ApiResult, ok};
use crate::core::state::AppState;

const MSG_LISTED: &str = "Jadwal berhasil diambil";

/// Own rows only: the schedule is keyed by the token's email and active group.
pub async fn mine(
    ValidatedQuery(query): ValidatedQuery<ScheduleQuery>,
    user: CurrentUser,
    State(state): State<AppState>,
) -> ApiResult<Vec<ScheduleItem>> {
    let (from, to) = query
        .resolve(Local::now().date_naive())
        .map_err(|errors| ApiError::invalid_input(errors.to_string()))?;
    let Some(group_uid) = user.group_uid.as_deref().filter(|g| !g.is_empty()) else {
        return ok(Vec::new(), MSG_LISTED);
    };
    let items = repository::list(&state.db, group_uid, &user.email, from, to).await?;
    ok(items, MSG_LISTED)
}
