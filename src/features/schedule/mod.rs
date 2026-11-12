//! Schedules: `GET /schedules/me` for everyone, management (list, create, bulk, update, delete)
//! for `ADMIN` and `SUPER_ADMIN`.

mod dto;
mod entity;
mod generate_dto;
mod generate_handlers;
mod generate_repository;
mod generator;
mod handlers;
mod manage_dto;
mod manage_handlers;
mod manage_plan;
mod manage_repository;
mod repository;

use axum::Router;
use axum::routing::{get, post};

use crate::core::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .merge(generate_handlers::generate_router())
        .route("/schedules/me", get(handlers::mine))
        .route(
            "/schedules",
            get(manage_handlers::list)
                .post(manage_handlers::create)
                .delete(manage_handlers::delete_batch),
        )
        .route("/schedules/bulk", post(manage_handlers::bulk))
        .route(
            "/schedules/{id}",
            axum::routing::put(manage_handlers::update).delete(manage_handlers::delete_one),
        )
}
