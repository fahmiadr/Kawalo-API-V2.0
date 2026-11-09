use axum::Router;

use crate::core::state::AppState;

/// WebSocket routes, mounted at the root (outside `/api/v1`).
pub fn ws_router() -> Router<AppState> {
    Router::new()
}
