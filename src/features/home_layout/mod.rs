//! Home screen ("Beranda") card layout, stored per user and per active group (`tkhomelayout`).

mod dto;
mod handlers;
mod repository;

use axum::Router;
use axum::routing::get;

use crate::core::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/me/home-layout", get(handlers::get).put(handlers::put))
}
