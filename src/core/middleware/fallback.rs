//! Global fallbacks: unknown routes and panics.

use std::any::Any;

use axum::response::{IntoResponse, Response};
use serde_json::Value;

use crate::core::config::Config;
use crate::core::error::ApiError;

/// 404 envelope for unknown paths and for methods a path does not serve (Elysia has no 405).
pub async fn not_found() -> ApiError {
    tracing::warn!("route not found");
    ApiError::route_not_found()
}

/// 500 envelope for a panicking handler; the panic message is a detail outside production.
pub fn panic_response(payload: Box<dyn Any + Send + 'static>) -> Response {
    let message = payload
        .downcast_ref::<&str>()
        .map(|s| (*s).to_owned())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "handler panicked".to_owned());
    tracing::error!("handler panicked: {message}");
    let mut err = ApiError::server();
    if !Config::global().is_production() {
        err = err.with_details(Value::String(message));
    }
    err.into_response()
}
