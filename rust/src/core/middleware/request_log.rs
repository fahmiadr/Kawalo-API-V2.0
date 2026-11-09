//! Correlation id and request logging, ported from `request-logger.middleware.ts`.

use std::time::Instant;

use axum::extract::{Request, State};
use axum::http::{HeaderValue, Method};
use axum::middleware::Next;
use axum::response::Response;
use uuid::Uuid;

use crate::core::client_ip::client_ip_from;
use crate::core::state::AppState;

const REQUEST_ID_HEADER: &str = "x-request-id";

/// Correlation id of the current request, available as a request extension.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestId(pub String);

/// Takes `x-request-id` from the request or mints a UUID, echoes it on the response, and logs one
/// line per request (status >= 500 as error, >= 400 as warning). `/health` is not logged.
/// `OPTIONS` requests pass untouched: the CORS plugin answers them before the TypeScript logger runs,
/// so they get no id and no log line.
pub async fn request_log(State(state): State<AppState>, mut req: Request, next: Next) -> Response {
    if req.method() == Method::OPTIONS {
        return next.run(req).await;
    }
    let started = Instant::now();
    let request_id = req
        .headers()
        .get(REQUEST_ID_HEADER)
        .and_then(|v| v.to_str().ok())
        .filter(|v| !v.is_empty())
        .map_or_else(|| Uuid::new_v4().to_string(), str::to_owned);
    if let Ok(value) = HeaderValue::from_str(&request_id) {
        req.headers_mut().insert(REQUEST_ID_HEADER, value);
    }
    req.extensions_mut().insert(RequestId(request_id.clone()));

    let method = req.method().clone();
    let path = req.uri().path().to_owned();
    let client_ip = client_ip_from(
        req.headers(),
        req.extensions(),
        &state.config.trusted_proxy_cidrs,
    );

    let mut response = next.run(req).await;
    if let Ok(value) = HeaderValue::from_str(&request_id) {
        response.headers_mut().insert(REQUEST_ID_HEADER, value);
    }

    if path != "/health" {
        let status = response.status().as_u16();
        let duration_ms = started.elapsed().as_secs_f64() * 1000.0;
        let message =
            format!("{method} {path} {status} ({duration_ms:.2}ms) [{request_id}] [{client_ip}]");
        if status >= 500 {
            tracing::error!(%request_id, %client_ip, status, duration_ms, "{message}");
        } else if status >= 400 {
            tracing::warn!(%request_id, %client_ip, status, duration_ms, "{message}");
        } else {
            tracing::info!(%request_id, %client_ip, status, duration_ms, "{message}");
        }
    }
    response
}
