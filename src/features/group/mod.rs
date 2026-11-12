//! `GET /groups/members`: joined members of the caller's active group.

mod collate;
mod repository;

use axum::Router;
use axum::extract::State;
use axum::routing::get;
use serde::Serialize;

use crate::core::extract::CurrentUser;
use crate::core::response::{ApiResult, ok};
use crate::core::state::AppState;

const MSG_LISTED: &str = "Daftar anggota grup berhasil diambil";

/// `profilePic` is camelCase on the wire, unlike the rest of the API.
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct MemberItem {
    pub email: String,
    pub name: Option<String>,
    #[serde(rename = "profilePic")]
    pub profile_pic: Option<String>,
    /// Same value as `profilePic`, under the name the rest of the API uses.
    pub avatar_url: Option<String>,
    /// Upper-cased `tkusergroup.Category` in this group.
    pub role: String,
}

pub fn router() -> Router<AppState> {
    Router::new().route("/groups/members", get(members))
}

async fn members(user: CurrentUser, State(state): State<AppState>) -> ApiResult<Vec<MemberItem>> {
    let Some(group_uid) = user.group_uid.as_deref().filter(|g| !g.is_empty()) else {
        return ok(Vec::new(), MSG_LISTED);
    };
    ok(repository::members(&state.db, group_uid).await?, MSG_LISTED)
}
