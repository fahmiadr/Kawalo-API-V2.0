//! `/ptt/recordings`: list, authenticated audio download, admin delete.

use std::path::{Component, Path, PathBuf};

use axum::body::Body;
use axum::extract::{FromRequestParts, Path as PathParam, State};
use axum::http::request::Parts;
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::Response;
use chrono::{SubsecRound, Utc};
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio_util::io::ReaderStream;

use super::dto::{ListQuery, RecordingItem, RecordingPage, encode_cursor};
use super::repository::{self, RecordingRow};
use crate::core::error::ApiError;
use crate::core::extract::{CurrentUser, ValidatedQuery, ValidationErrors};
use crate::core::response::{ApiResult, ok};
use crate::core::state::AppState;

const MSG_NOT_FOUND: &str = "Rekaman tidak ditemukan.";
const STREAM_CHUNK_BYTES: usize = 64 * 1024;

fn not_found() -> ApiError {
    ApiError::not_found().with_message(MSG_NOT_FOUND)
}

/// The `{id}` path segment, 1 to 64 characters like the incident id.
#[derive(Debug)]
pub struct RecordingId(String);

impl<S: Send + Sync> FromRequestParts<S> for RecordingId {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let PathParam(id) = PathParam::<String>::from_request_parts(parts, state)
            .await
            .map_err(|_| ApiError::invalid_input("/id: Expected required property"))?;
        let mut errors = ValidationErrors::new();
        errors.length("id", &id, 1..=64);
        errors
            .into_result()
            .map_err(|errors| ApiError::invalid_input(errors.to_string()))?;
        Ok(Self(id))
    }
}

pub async fn list(
    ValidatedQuery(query): ValidatedQuery<ListQuery>,
    user: CurrentUser,
    State(state): State<AppState>,
) -> ApiResult<RecordingPage> {
    const MESSAGE: &str = "Riwayat rekaman PTT berhasil diambil";
    let group_uid = user.require_group_uid()?;
    let take = query.take();
    let mut rows =
        repository::list(&state.db, group_uid, query.cursor().as_ref(), take + 1).await?;
    let has_more = rows.len() > take as usize;
    rows.truncate(take as usize);
    let next_before = has_more
        .then(|| rows.last())
        .flatten()
        .map(|last| encode_cursor(last.started_at, &last.id));
    ok(
        RecordingPage {
            items: rows.into_iter().map(RecordingItem::from).collect(),
            next_before,
        },
        MESSAGE,
    )
}

/// Same rules as the incident delete: the active group first, then `allow_admin`, then a
/// missing or already deleted row is a 404.
pub async fn remove(
    id: RecordingId,
    user: CurrentUser,
    State(state): State<AppState>,
) -> ApiResult<()> {
    let group_uid = user.require_group_uid()?;
    if !user.allow_admin {
        return Err(
            ApiError::forbidden().with_message("Hanya administrator yang dapat menghapus rekaman.")
        );
    }
    let row = repository::find(&state.db, &id.0, group_uid)
        .await?
        .ok_or_else(not_found)?;
    repository::soft_delete(&state.db, &row.id, Utc::now().trunc_subsecs(0)).await?;
    // Best effort: the tombstone already hides the recording; a left-over file is for a future
    // retention job.
    if let Some(location) = resolve(&state.config.upload_dir, &row.file_path) {
        if let Err(err) = tokio::fs::remove_file(&location).await
            && err.kind() != std::io::ErrorKind::NotFound
        {
            tracing::warn!(error = %err, "[PTT] could not remove the file of recording {}", row.id);
        }
    } else {
        tracing::warn!(
            "[PTT] recording {} has an unsafe file path; left alone",
            row.id
        );
    }
    ok((), "Rekaman berhasil dihapus")
}

/// The file location for a stored relative path, only when it is plain `a/b/c` segments (the
/// uploader never produces anything else), so it cannot leave `root`.
fn resolve(root: &Path, relative: &str) -> Option<PathBuf> {
    let path = Path::new(relative);
    (path.components().count() > 0 && path.components().all(|c| matches!(c, Component::Normal(_))))
        .then(|| root.join(path))
}

pub async fn audio(
    id: RecordingId,
    user: CurrentUser,
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let group_uid = user.require_group_uid()?;
    let row: RecordingRow = repository::find(&state.db, &id.0, group_uid)
        .await?
        .ok_or_else(not_found)?;
    let location = resolve(&state.config.upload_dir, &row.file_path).ok_or_else(not_found)?;
    let mut file = File::open(&location)
        .await
        .map_err(|err| match err.kind() {
            std::io::ErrorKind::NotFound => not_found(),
            _ => ApiError::internal(err),
        })?;
    let total = file.metadata().await.map_err(ApiError::internal)?.len();

    let range = match headers.get(header::RANGE) {
        None => None,
        Some(value) => match parse_range(value.to_str().unwrap_or(""), total) {
            RangeOutcome::Ignore => None,
            RangeOutcome::Satisfiable(start, end) => Some((start, end)),
            RangeOutcome::Unsatisfiable => {
                let mut response = Response::new(Body::empty());
                *response.status_mut() = StatusCode::RANGE_NOT_SATISFIABLE;
                set_common(response.headers_mut());
                response.headers_mut().insert(
                    header::CONTENT_RANGE,
                    HeaderValue::from_str(&format!("bytes */{total}"))
                        .map_err(ApiError::internal)?,
                );
                return Ok(response);
            }
        },
    };

    let (status, start, length) = match range {
        Some((start, end)) => (StatusCode::PARTIAL_CONTENT, start, end - start + 1),
        None => (StatusCode::OK, 0, total),
    };
    if start > 0 {
        file.seek(std::io::SeekFrom::Start(start))
            .await
            .map_err(ApiError::internal)?;
    }
    let body = Body::from_stream(ReaderStream::with_capacity(
        file.take(length),
        STREAM_CHUNK_BYTES,
    ));
    let mut response = Response::new(body);
    *response.status_mut() = status;
    let out = response.headers_mut();
    set_common(out);
    out.insert(header::CONTENT_LENGTH, HeaderValue::from(length));
    if let Some((start, end)) = range {
        out.insert(
            header::CONTENT_RANGE,
            HeaderValue::from_str(&format!("bytes {start}-{end}/{total}"))
                .map_err(ApiError::internal)?,
        );
    }
    Ok(response)
}

fn set_common(headers: &mut HeaderMap) {
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("audio/ogg"));
    headers.insert(header::ACCEPT_RANGES, HeaderValue::from_static("bytes"));
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("private"));
}

#[derive(Debug, PartialEq, Eq)]
enum RangeOutcome {
    /// Absent unit, several ranges or garbage: serve the whole file, as RFC 9110 allows.
    Ignore,
    /// Inclusive byte positions.
    Satisfiable(u64, u64),
    Unsatisfiable,
}

/// A single `bytes=a-b`, `bytes=a-` or `bytes=-n` range against a file of `total` bytes.
fn parse_range(value: &str, total: u64) -> RangeOutcome {
    let Some(spec) = value.trim().strip_prefix("bytes=") else {
        return RangeOutcome::Ignore;
    };
    if spec.contains(',') {
        return RangeOutcome::Ignore;
    }
    let Some((first, last)) = spec.trim().split_once('-') else {
        return RangeOutcome::Ignore;
    };
    let (first, last) = (first.trim(), last.trim());
    let parse = |s: &str| s.parse::<u64>().ok();
    match (first.is_empty(), last.is_empty()) {
        (true, true) => RangeOutcome::Ignore,
        // Suffix: the last n bytes.
        (true, false) => match parse(last) {
            None => RangeOutcome::Ignore,
            Some(0) => RangeOutcome::Unsatisfiable,
            Some(_) if total == 0 => RangeOutcome::Unsatisfiable,
            Some(n) => RangeOutcome::Satisfiable(total.saturating_sub(n), total - 1),
        },
        (false, _) => {
            let Some(start) = parse(first) else {
                return RangeOutcome::Ignore;
            };
            let end = match (last.is_empty(), parse(last)) {
                (true, _) => None,
                (false, Some(end)) => Some(end),
                (false, None) => return RangeOutcome::Ignore,
            };
            // `a-b` with b < a is syntactically invalid.
            if end.is_some_and(|end| end < start) {
                return RangeOutcome::Ignore;
            }
            if start >= total {
                return RangeOutcome::Unsatisfiable;
            }
            RangeOutcome::Satisfiable(start, end.map_or(total - 1, |end| end.min(total - 1)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::RangeOutcome::*;
    use super::*;

    #[test]
    fn ranges_follow_rfc_9110() {
        assert_eq!(parse_range("bytes=0-9", 100), Satisfiable(0, 9));
        assert_eq!(parse_range("bytes=90-", 100), Satisfiable(90, 99));
        assert_eq!(parse_range("bytes=-10", 100), Satisfiable(90, 99));
        assert_eq!(parse_range("bytes=-500", 100), Satisfiable(0, 99));
        assert_eq!(parse_range("bytes=50-5000", 100), Satisfiable(50, 99));
        assert_eq!(parse_range("bytes=100-", 100), Unsatisfiable);
        assert_eq!(parse_range("bytes=100-200", 100), Unsatisfiable);
        assert_eq!(parse_range("bytes=-0", 100), Unsatisfiable);
        assert_eq!(parse_range("bytes=0-", 0), Unsatisfiable);
        assert_eq!(parse_range("bytes=9-3", 100), Ignore);
        assert_eq!(parse_range("bytes=0-1,5-6", 100), Ignore);
        assert_eq!(parse_range("items=0-1", 100), Ignore);
        assert_eq!(parse_range("bytes=a-b", 100), Ignore);
        assert_eq!(parse_range("bytes=-", 100), Ignore);
    }

    #[test]
    fn stored_paths_cannot_leave_the_upload_root() {
        let root = Path::new("/srv/uploads");
        assert_eq!(
            resolve(root, "ptt/2026/11/a.opus"),
            Some(PathBuf::from("/srv/uploads/ptt/2026/11/a.opus"))
        );
        for bad in ["../x", "/etc/passwd", "ptt/../../x", "./a", ""] {
            assert_eq!(resolve(root, bad), None, "{bad}");
        }
    }
}
