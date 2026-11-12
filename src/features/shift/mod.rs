//! Shift management (`tkshift`): list for every member, create, replace and soft-delete for admins.

mod dto;
mod handlers;
mod repository;

use axum::Router;
use axum::routing::{get, put};

use crate::core::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/shifts", get(handlers::list).post(handlers::create))
        .route(
            "/shifts/{uid}",
            put(handlers::update).delete(handlers::delete_one),
        )
}
