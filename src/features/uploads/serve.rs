//! Public file serving for `GET /uploads/{yyyy}/{mm}/{uuid}.{ext}`.

use axum::body::Body;
use axum::extract::rejection::PathRejection;
use axum::extract::{Path, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::Response;
use tokio::fs::File;
use tokio_util::io::ReaderStream;
use uuid::{Uuid, Version};

use super::format::Format;
use crate::core::error::ApiError;
use crate::core::state::AppState;

const STREAM_CHUNK_BYTES: usize = 64 * 1024;

fn not_found() -> ApiError {
    ApiError::not_found().with_message("Berkas tidak ditemukan.")
}

/// Accepts exactly the paths the uploader produces: `yyyy`, `01`..`12`, and a lower-case
/// hyphenated UUID v4 plus a known extension. Anything else (`..`, separators, dot files) is a 404.
fn parse(year: &str, month: &str, name: &str) -> Option<Format> {
    let digits = |s: &str, len: usize| s.len() == len && s.bytes().all(|b| b.is_ascii_digit());
    if !digits(year, 4) || !digits(month, 2) || !(1..=12).contains(&month.parse::<u8>().ok()?) {
        return None;
    }
    let (stem, ext) = name.split_once('.')?;
    let uuid = Uuid::try_parse(stem).ok()?;
    let canonical = stem.len() == 36 && stem.bytes().all(|b| !b.is_ascii_uppercase());
    (canonical && uuid.get_version() == Some(Version::Random)).then_some(())?;
    Format::from_ext(ext)
}

pub async fn file(
    State(state): State<AppState>,
    path: Result<Path<(String, String, String)>, PathRejection>,
) -> Result<Response, ApiError> {
    let Ok(Path((year, month, name))) = path else {
        return Err(not_found());
    };
    let format = parse(&year, &month, &name).ok_or_else(not_found)?;

    let location = state.config.upload_dir.join(&year).join(&month).join(&name);
    let file = File::open(&location)
        .await
        .map_err(|err| match err.kind() {
            std::io::ErrorKind::NotFound => not_found(),
            _ => ApiError::internal(err),
        })?;
    let meta = file.metadata().await.map_err(ApiError::internal)?;
    if !meta.is_file() {
        return Err(not_found());
    }

    let disposition = if format.is_image() {
        "inline"
    } else {
        "attachment"
    };
    let body = Body::from_stream(ReaderStream::with_capacity(file, STREAM_CHUNK_BYTES));
    let mut response = Response::new(body);
    *response.status_mut() = StatusCode::OK;
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static(format.content_type()),
    );
    headers.insert(header::CONTENT_LENGTH, HeaderValue::from(meta.len()));
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=31536000, immutable"),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_static(disposition),
    );
    // Uploaded content is never allowed to run or load anything if a browser renders it.
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static("default-src 'none'; sandbox"),
    );
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "3f2b8c1e-7a4d-4e9b-9c55-0a1b2c3d4e5f";

    #[test]
    fn accepts_only_the_uploader_layout() {
        assert_eq!(
            parse("2026", "11", &format!("{ID}.webp")),
            Some(Format::Webp)
        );
        assert_eq!(parse("2026", "01", &format!("{ID}.pdf")), Some(Format::Pdf));
    }

    #[test]
    fn rejects_everything_else() {
        for (y, m, n) in [
            ("..", "11", format!("{ID}.webp")),
            ("2026", "..", format!("{ID}.webp")),
            ("2026", "13", format!("{ID}.webp")),
            ("2026", "00", format!("{ID}.webp")),
            ("202", "11", format!("{ID}.webp")),
            ("2026", "11", "..".to_owned()),
            ("2026", "11", format!(".{ID}.webp")),
            ("2026", "11", format!("{ID}.exe")),
            ("2026", "11", format!("{ID}.webp.exe")),
            ("2026", "11", ID.to_owned()),
            ("2026", "11", format!("{}.webp", ID.to_uppercase())),
            ("2026", "11", format!("{{{ID}}}.webp")),
            (
                "2026",
                "11",
                "3f2b8c1e7a4d4e9b9c550a1b2c3d4e5f.webp".to_owned(),
            ),
            (
                "2026",
                "11",
                "019349a0-1111-7000-8000-000000000000.webp".to_owned(),
            ),
            ("2026", "11", format!("a/../{ID}.webp")),
        ] {
            assert_eq!(parse(y, m, &n), None, "{y}/{m}/{n}");
        }
    }
}
