//! Read, play and delete recorded push-to-talk turns (`/ptt/recordings`). Recording itself is in
//! `super::recorder`. There is no TypeScript counterpart.

mod dto;
mod handlers;
pub(super) mod repository;

use axum::Router;
use axum::routing::{get, put};

use crate::core::state::AppState;

/// Mounted under `/api/v1`.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/ptt/recordings", get(handlers::list))
        .route(
            "/ptt/recordings/{id}",
            axum::routing::delete(handlers::remove),
        )
        .route("/ptt/recordings/{id}/audio", get(handlers::audio))
        .route(
            "/ptt/recordings/by-request/{id}/transcript",
            put(handlers::put_transcript),
        )
}
