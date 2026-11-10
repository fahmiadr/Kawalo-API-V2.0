//! `GET /schedules/me`: the caller's own shift schedule in a date range.

mod dto;
mod entity;
mod handlers;
mod repository;

use axum::Router;
use axum::routing::get;

use crate::core::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/schedules/me", get(handlers::mine))
}
