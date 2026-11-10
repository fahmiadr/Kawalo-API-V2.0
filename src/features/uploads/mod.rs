//! File uploads: `POST /api/v1/uploads` (authenticated) and public `GET /uploads/...`.
//!
//! There is no TypeScript counterpart. Files live on disk under `UPLOAD_DIR`; nothing is stored in
//! the database, so orphaned files are never cleaned up automatically.

mod format;
mod serve;
mod upload;

use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::routing::{get, post};

use crate::core::state::AppState;

/// Multipart framing around the largest accepted document.
const BODY_LIMIT_BYTES: usize = format::DOCUMENT_MAX_BYTES + 64 * 1024;

/// Mounted under `/api/v1`.
pub fn router() -> Router<AppState> {
    // Route-level limit: overrides the global 10 MiB for this route only.
    Router::new().route(
        "/uploads",
        post(upload::upload).layer(DefaultBodyLimit::max(BODY_LIMIT_BYTES)),
    )
}

/// Mounted at the root: stored files are served outside `/api/v1`.
pub fn files_router() -> Router<AppState> {
    Router::new().route("/uploads/{year}/{month}/{name}", get(serve::file))
}
