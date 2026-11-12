//! Router assembly, ported from `app.ts`.

use std::sync::Arc;

use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::middleware::{from_fn, from_fn_with_state};
use axum::routing::get;
use tower::ServiceBuilder;
use tower_http::catch_panic::CatchPanicLayer;

use crate::core::constants::default_api_prefix;
use crate::core::middleware::cors::{self, Origins};
use crate::core::middleware::fallback::{not_found, panic_response};
use crate::core::middleware::path::{strip_allow, trim_trailing_slash};
use crate::core::middleware::rate_limit::{self, RateLimitState, RateLimiter};
use crate::core::middleware::request_log::request_log;
use crate::core::middleware::security_headers::security_headers;
use crate::core::state::AppState;
use crate::core::{health, security::jwks};
use crate::features;

/// Largest accepted request body.
const BODY_LIMIT_BYTES: usize = 10 * 1024 * 1024;

/// Builds the full application: probes, JWKS, `/api/v1` features, the PTT WebSocket, and the
/// layers (request id and logging, CORS, security headers, rate limit, panic handling).
///
/// Serve it with `into_make_service_with_connect_info::<SocketAddr>()` so rate limiting and
/// logging see the real peer address.
pub fn build_app(state: AppState) -> Router {
    build_app_with(state, Router::new())
}

/// [`build_app`] with extra root-level routes, so tests can exercise extractors and layers without
/// a real feature.
#[doc(hidden)]
pub fn build_app_with(state: AppState, extra: Router<AppState>) -> Router {
    health::mark_started();

    let v1 = Router::new()
        .merge(features::auth::router())
        .merge(features::group::router())
        .merge(features::guest::router())
        .merge(features::operational::router())
        .merge(features::attendance::router())
        .merge(features::faces::router())
        .merge(features::schedule::router())
        .merge(features::shift::router())
        .merge(features::information::router())
        .merge(features::incident::router())
        .merge(features::ptt::recordings_router())
        .merge(features::ptt::members_router())
        .merge(features::uploads::router());

    let limiter = RateLimitState {
        limiter: Arc::new(RateLimiter::new(
            state.config.rate_limit_max,
            state.config.rate_limit_duration,
        )),
        app: state.clone(),
    };
    let origins = Arc::new(Origins::parse(&state.config.cors_origin));

    let app = Router::new()
        .route("/health", get(health::live))
        .route("/health/ready", get(health::ready))
        .route("/.well-known/jwks.json", get(jwks::jwks))
        .merge(features::ptt::ws_router())
        .merge(features::uploads::files_router())
        .merge(extra)
        .nest(&default_api_prefix(), v1)
        .fallback(not_found)
        .method_not_allowed_fallback(not_found)
        .layer(DefaultBodyLimit::max(BODY_LIMIT_BYTES))
        .layer(CatchPanicLayer::custom(panic_response))
        .layer(from_fn_with_state(limiter, rate_limit::rate_limit))
        .layer(from_fn(security_headers))
        .layer(from_fn_with_state(origins, cors::cors))
        .layer(from_fn_with_state(state.clone(), request_log))
        .with_state(state);

    // Routing happens before any layer and axum adds `allow` after them, so both fixes wrap the
    // whole router.
    Router::new().fallback_service(
        ServiceBuilder::new()
            .map_request(trim_trailing_slash)
            .map_response(strip_allow)
            .service(app),
    )
}
