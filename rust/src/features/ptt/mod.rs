//! Push-to-talk over WebSocket, ported from `features/ptt` (`ptt.ws.ts`, `ptt.controller.ts`,
//! the room registry, the Redis floor lock and the cluster relay).
//!
//! One speaker per group holds the floor at a time (a Redis lock with a 5 s lease that audio
//! renews); an admin may take it from a guard. Audio frames are never decoded: they are handed as
//! refcounted buffers to every other socket of the group, locally and, through Redis pub/sub, on
//! other instances.

use std::sync::Arc;
use std::time::Duration;

use axum::Extension;
use axum::Router;
use axum::routing::get;

use crate::core::state::AppState;

mod frames;
mod registry;
mod relay;
mod session;
mod store;
mod ws;

/// WebSocket routes, mounted at the root (outside `/api/v1`). Each call creates its own socket
/// registry and relay, so two apps in one process behave like two instances of a cluster.
pub fn ws_router() -> Router<AppState> {
    ws_router_with_tot(session::TOT_DURATION)
}

/// [`ws_router`] with a shorter time-out timer, for tests.
#[doc(hidden)]
pub fn ws_router_with_tot(tot_duration: Duration) -> Router<AppState> {
    Router::new()
        .route("/ws/ptt/{group_uid}", get(ws::upgrade))
        .layer(Extension(Arc::new(ws::HubCell::new(tot_duration))))
}
