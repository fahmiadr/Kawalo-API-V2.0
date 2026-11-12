//! File uploads: `POST /api/v1/uploads` (authenticated) and public `GET /uploads/...`.
//!
//! There is no TypeScript counterpart. Files live on disk under `UPLOAD_DIR`; nothing is stored in
//! the database, so orphaned files are never cleaned up automatically.

mod format;
mod serve;
mod upload;

use std::path::PathBuf;

use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::http::HeaderMap;
use axum::routing::{get, post};

use crate::core::config::Config;
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

/// Where `url` is stored on disk, when it is exactly an image URL this server hands out for an
/// upload (same base as `POST /uploads` would answer the request with). Existence is not checked.
pub fn own_image_path(config: &Config, headers: &HeaderMap, url: &str) -> Option<PathBuf> {
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return None;
    }
    let rest = url
        .strip_prefix(upload::base_url(config, headers).as_str())?
        .strip_prefix("/uploads/")?;
    let mut segments = rest.split('/');
    let (year, month, name) = (segments.next()?, segments.next()?, segments.next()?);
    let image = segments.next().is_none() && serve::parse(year, month, name)?.is_image();
    image.then(|| config.upload_dir.join(year).join(month).join(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn own_image_path_accepts_only_this_servers_image_urls() {
        let config = Config {
            public_base_url: "https://api.kawalo.test".to_owned(),
            ..Config::default()
        };
        let headers = HeaderMap::new();
        let id = "0b6b5a4e-8a52-4f0e-9c3a-2f1d7c9e4b10";
        let good = format!("https://api.kawalo.test/uploads/2026/11/{id}.webp");
        assert_eq!(
            own_image_path(&config, &headers, &good),
            Some(
                config
                    .upload_dir
                    .join("2026")
                    .join("11")
                    .join(format!("{id}.webp"))
            )
        );
        for bad in [
            format!("https://evil.test/uploads/2026/11/{id}.webp"),
            format!("https://api.kawalo.test.evil.test/uploads/2026/11/{id}.webp"),
            format!("http://api.kawalo.test/uploads/2026/11/{id}.webp"),
            format!("https://api.kawalo.test/uploads/2026/11/{id}.pdf"),
            format!("https://api.kawalo.test/uploads/2026/11/{id}.webp?x=1"),
            format!("https://api.kawalo.test/uploads/2026/11/{id}.webp/x"),
            format!("https://api.kawalo.test/uploads/2026/../{id}.webp"),
            format!("https://api.kawalo.test/uploads/ptt/11/{id}.webp"),
            format!("https://api.kawalo.test/api/v1/uploads/2026/11/{id}.webp"),
            "javascript:alert(1)".to_owned(),
            String::new(),
        ] {
            assert_eq!(own_image_path(&config, &headers, &bad), None, "{bad}");
        }
    }
}
