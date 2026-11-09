//! Incident reports with realtime reply threads (`/incidents`), ported from `src/features/incident`.

mod bus;
mod dto;
mod event;
mod handlers;
mod repository;
mod sse;

use std::sync::Arc;

use axum::routing::{get, patch, post};
use axum::{Extension, Router};

use crate::core::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/incidents",
            get(handlers::list).post(handlers::create_incident),
        )
        .route(
            "/incidents/",
            get(handlers::list).post(handlers::create_incident),
        )
        .route(
            "/incidents/{id}",
            get(handlers::detail).delete(handlers::remove),
        )
        .route("/incidents/{id}/stream", get(handlers::stream))
        .route("/incidents/{id}/replies", post(handlers::reply))
        .route("/incidents/{id}/resolve", patch(handlers::resolve))
        .layer(Extension(Arc::new(bus::IncidentBus::default())))
}
