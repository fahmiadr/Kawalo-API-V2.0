use axum::extract::State;

use super::dto::{Layout, LayoutBody, LayoutData};
use super::repository;
use crate::core::error::ApiError;
use crate::core::extract::{AuthedJson, CurrentUser};
use crate::core::response::{ApiResult, ok};
use crate::core::state::AppState;

pub async fn get(user: CurrentUser, State(state): State<AppState>) -> ApiResult<LayoutData> {
    let group_uid = user.require_group_uid()?;
    let stored = repository::find(&state.db, &user.email, group_uid).await?;
    // An unreadable row falls back to the default rather than locking the user out of the home.
    let data = match stored.and_then(|raw| serde_json::from_str::<Layout>(&raw).ok()) {
        Some(layout) => LayoutData {
            layout,
            is_default: false,
        },
        None => LayoutData {
            layout: Layout::default_layout(),
            is_default: true,
        },
    };
    ok(data, "Tata letak beranda berhasil diambil")
}

pub async fn put(
    State(state): State<AppState>,
    AuthedJson(auth, body): AuthedJson<LayoutBody>,
) -> ApiResult<LayoutData> {
    // Validated before auth, like every body-carrying route.
    let layout = body.validate()?;
    let user = auth.require_auth()?;
    let group_uid = user.require_group_uid()?;
    let raw = serde_json::to_string(&layout).map_err(ApiError::internal)?;
    repository::upsert(&state.db, &user.email, group_uid, &raw).await?;
    ok(
        LayoutData {
            layout,
            is_default: false,
        },
        "Tata letak beranda berhasil disimpan",
    )
}
