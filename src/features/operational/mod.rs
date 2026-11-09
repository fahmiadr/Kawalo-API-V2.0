//! `GET /operational`: duty journeys of the caller's group, one card per person and day.

mod entity;
mod repository;

use axum::Router;
use axum::extract::State;
use axum::routing::get;

use crate::core::extract::{CurrentUser, ValidatedQuery};
use crate::core::response::{ApiResult, ok};
use crate::core::state::AppState;
use crate::features::period_query::PeriodQuery;

use entity::OperationalItem;

const MSG_LISTED: &str = "Daftar aktifitas operasional berhasil diambil";

pub fn router() -> Router<AppState> {
    Router::new().route("/operational", get(list))
}

async fn list(
    ValidatedQuery(query): ValidatedQuery<PeriodQuery>,
    user: CurrentUser,
    State(state): State<AppState>,
) -> ApiResult<Vec<OperationalItem>> {
    let Some(group_uid) = user.group_uid.as_deref().filter(|g| !g.is_empty()) else {
        return ok(Vec::new(), MSG_LISTED);
    };
    let items = repository::list(&state.db, &query.into_filter(group_uid)).await?;
    ok(items, MSG_LISTED)
}
