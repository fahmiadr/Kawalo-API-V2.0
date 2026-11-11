//! `GET /ptt/members`: who is connected to the channel of the caller's active group.

use axum::Router;
use axum::extract::State;
use axum::routing::get;
use serde::Serialize;

use super::store::FloorStore;
use crate::core::error::ApiError;
use crate::core::extract::CurrentUser;
use crate::core::response::{ApiResult, ok};
use crate::core::state::AppState;

#[derive(Serialize)]
pub struct MemberItem {
    user_id: String,
    name: String,
    role: String,
    speaking: bool,
}

#[derive(Serialize)]
pub struct MemberList {
    members: Vec<MemberItem>,
}

pub fn router() -> Router<AppState> {
    Router::new().route("/ptt/members", get(list))
}

async fn list(user: CurrentUser, State(state): State<AppState>) -> ApiResult<MemberList> {
    let group = user.require_group_uid()?;
    let store = FloorStore::new(state.redis.clone());
    let members = store.members(group).await.map_err(ApiError::internal)?;
    let floor = store
        .current_floor(group)
        .await
        .map_err(ApiError::internal)?;
    let speaker = floor.as_ref().map(|s| s.user_id.as_str());
    ok(
        MemberList {
            members: members
                .into_iter()
                .map(|m| MemberItem {
                    speaking: speaker == Some(m.user_id.as_str()),
                    user_id: m.user_id,
                    name: m.name,
                    role: m.role,
                })
                .collect(),
        },
        "Anggota saluran PTT berhasil diambil",
    )
}
