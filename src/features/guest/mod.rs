//! Guest book (`/guests`), ported from `src/features/guest` on `legacy/typescript`.

mod dto;
mod handlers;
mod repository;

use axum::Router;
use axum::routing::{get, patch};

use crate::core::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/guests",
            get(handlers::list)
                .post(handlers::create)
                .delete(handlers::delete_batch),
        )
        .route(
            "/guests/master-types",
            get(handlers::master_types)
                .put(handlers::update)
                .patch(handlers::update)
                .delete(handlers::delete_one),
        )
        .route(
            "/guests/{id}",
            get(handlers::get_by_id)
                .put(handlers::update)
                .patch(handlers::update)
                .delete(handlers::delete_one),
        )
        .route("/guests/{id}/acknowledge", patch(handlers::acknowledge))
        .route("/guests/{id}/checkout", patch(handlers::checkout))
}
