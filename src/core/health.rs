//! `/health` (liveness) and `/health/ready` (readiness) probes.

use std::sync::OnceLock;
use std::time::Instant;

use axum::extract::State;
use axum::http::StatusCode;
use chrono::Utc;
use serde_json::{Value, json};

use crate::core::clean::iso_string;
use crate::core::constants::APP_NAME;
use crate::core::db;
use crate::core::error::{ApiError, ErrorCode};
use crate::core::redis;
use crate::core::response::{ApiResult, ok};
use crate::core::state::AppState;

static STARTED: OnceLock<Instant> = OnceLock::new();

/// Marks process start for the `uptime` field; call early in `main`.
pub fn mark_started() {
    STARTED.get_or_init(Instant::now);
}

fn uptime_seconds() -> f64 {
    STARTED.get_or_init(Instant::now).elapsed().as_secs_f64()
}

fn timestamp() -> String {
    iso_string(Utc::now())
}

pub async fn live() -> ApiResult<Value> {
    ok(
        json!({
            "app": APP_NAME,
            "status": "ok",
            "uptime": uptime_seconds(),
            "timestamp": timestamp(),
        }),
        "Sistem berjalan normal",
    )
}

pub async fn ready(State(state): State<AppState>) -> ApiResult<Value> {
    let (database, redis) = tokio::join!(
        db::health_check(&state.db),
        redis::health_check(&state.redis)
    );

    if !(database.is_healthy() && redis.is_healthy()) {
        return Err(ApiError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            ErrorCode::ServiceUnavailable,
            "Sistem belum siap melayani traffic",
        )
        .with_details(json!({
            "status": "not_ready",
            "database": database,
            "redis": redis,
            "timestamp": timestamp(),
        })));
    }

    ok(
        json!({
            "status": "ready",
            "database": database,
            "redis": redis,
            "uptime": uptime_seconds(),
            "timestamp": timestamp(),
        }),
        "Seluruh dependensi sistem siap melayani traffic",
    )
}
