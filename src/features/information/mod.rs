//! Information feed (`tkfeed`): list, create, update (PUT and PATCH) and delete (single and batch).

pub(super) mod dto;
mod handlers;
mod repository;

use axum::Router;
use axum::routing::{get, put};

use crate::core::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/information",
            get(handlers::list)
                .post(handlers::create)
                .delete(handlers::delete_batch),
        )
        .route(
            "/information/{id}",
            put(handlers::update)
                .patch(handlers::update)
                .delete(handlers::delete_one),
        )
}
