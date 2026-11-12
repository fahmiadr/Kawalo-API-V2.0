//! `/auth`: login, refresh, group switch, logout, own avatar and password change. The JWKS endpoint lives in `core::security`.

pub(super) mod dto;
mod handlers;
mod login_limiter;
mod password_check;
mod sessions;
mod user;

use axum::Router;
use axum::routing::{post, put};

use crate::core::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/auth/login", post(handlers::login))
        .route("/auth/refresh", post(handlers::refresh))
        .route("/auth/switch-group", post(handlers::switch_group))
        .route("/auth/logout", post(handlers::logout))
        .route("/auth/me/avatar", put(handlers::update_avatar))
        .route("/auth/change-password", post(handlers::change_password))
}
