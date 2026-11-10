//! `POST /api/v1/uploads`: validate, store atomically and hand back a public URL.

use std::io;
use std::path::Path;
use std::sync::LazyLock;

use axum::extract::multipart::{Field, MultipartError, MultipartRejection};
use axum::extract::{Multipart, State};
use axum::http::{HeaderMap, StatusCode, header};
use chrono::{Datelike, Utc};
use redis::Script;
use serde::Serialize;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use uuid::Uuid;

use super::format::{
    DOCUMENT_MAX_BYTES, Format, IMAGE_MAX_BYTES, extension, sanitize_name, validate_document,
    validate_webp,
};
use crate::core::config::Config;
use crate::core::error::{ApiError, ErrorCode};
use crate::core::extract::CurrentUser;
use crate::core::response::{ApiResult, created};
use crate::core::state::AppState;

/// Uploads per user per window, on top of the global per-IP limit.
const USER_LIMIT: i64 = 30;
const USER_WINDOW_SECONDS: i64 = 60;
/// Stored URLs end up in columns of at most 2048 characters.
const MAX_URL_LEN: usize = 2048;
const MAX_KIND_LEN: usize = 16;

/// Atomic fixed window; the TTL is set together with the first hit so a key can never outlive it.
static USER_WINDOW: LazyLock<Script> = LazyLock::new(|| {
    Script::new(
        "local n = redis.call('INCR', KEYS[1]) \
         if n == 1 then redis.call('EXPIRE', KEYS[1], ARGV[1]) end \
         return {n, redis.call('TTL', KEYS[1])}",
    )
});

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Image,
    Document,
}

impl Kind {
    fn parse(raw: &str) -> Option<Self> {
        match raw {
            "image" => Some(Self::Image),
            "document" => Some(Self::Document),
            _ => None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Image => "image",
            Self::Document => "document",
        }
    }

    fn max_bytes(self) -> usize {
        match self {
            Self::Image => IMAGE_MAX_BYTES,
            Self::Document => DOCUMENT_MAX_BYTES,
        }
    }

    fn too_large(self) -> ApiError {
        let message = match self {
            Self::Image => "Ukuran gambar maksimal 150 KB.",
            Self::Document => "Ukuran dokumen maksimal 10 MB.",
        };
        ApiError::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            ErrorCode::BadRequest,
            message,
        )
    }
}

#[derive(Debug, Serialize)]
pub struct Uploaded {
    url: String,
    kind: &'static str,
    size: usize,
    content_type: &'static str,
    /// Sanitized client file name, for display only.
    name: String,
}

fn unprocessable(message: &str) -> ApiError {
    ApiError::new(
        StatusCode::UNPROCESSABLE_ENTITY,
        ErrorCode::ValidationError,
        message,
    )
}

fn multipart_error(err: &MultipartError, kind: Option<Kind>) -> ApiError {
    if err.status() == StatusCode::PAYLOAD_TOO_LARGE {
        return kind.unwrap_or(Kind::Document).too_large();
    }
    unprocessable("Permintaan multipart tidak valid.")
}

async fn check_user_limit(state: &AppState, user: &CurrentUser) -> Result<(), ApiError> {
    let mut redis = state.redis.clone();
    let (count, ttl): (i64, i64) = USER_WINDOW
        .key(format!("upload:rate:{}", user.sub))
        .arg(USER_WINDOW_SECONDS)
        .invoke_async(&mut redis)
        .await?;
    if count <= USER_LIMIT {
        return Ok(());
    }
    let wait = u64::try_from(ttl.max(1)).unwrap_or(1);
    Err(ApiError::rate_limit(Some(wait)).with_message(format!(
        "Terlalu banyak unggahan. Coba lagi dalam {wait} detik."
    )))
}

/// Reads a field into memory, failing with `too_large` as soon as it passes `max_bytes`.
async fn read_capped(
    mut field: Field<'_>,
    max_bytes: usize,
    too_large: impl Fn() -> ApiError,
    kind: Option<Kind>,
) -> Result<Vec<u8>, ApiError> {
    let mut data = Vec::new();
    while let Some(chunk) = field
        .chunk()
        .await
        .map_err(|err| multipart_error(&err, kind))?
    {
        if data.len() + chunk.len() > max_bytes {
            return Err(too_large());
        }
        data.extend_from_slice(&chunk);
    }
    Ok(data)
}

struct Parts {
    kind: Kind,
    name: String,
    data: Vec<u8>,
}

async fn read_parts(mut multipart: Multipart) -> Result<Parts, ApiError> {
    let mut kind: Option<Kind> = None;
    let mut file: Option<(String, Vec<u8>)> = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|err| multipart_error(&err, kind))?
    {
        match field.name() {
            Some("kind") if kind.is_none() => {
                let raw = read_capped(field, MAX_KIND_LEN, kind_error, None).await?;
                kind = Some(
                    std::str::from_utf8(&raw)
                        .ok()
                        .and_then(|raw| Kind::parse(raw.trim()))
                        .ok_or_else(kind_error)?,
                );
            }
            Some("file") if file.is_none() => {
                let name = sanitize_name(field.file_name().unwrap_or_default());
                // Without a `kind` yet the larger limit applies; the kind's own limit is checked
                // again once it is known.
                let cap = kind.map_or(DOCUMENT_MAX_BYTES, Kind::max_bytes);
                let too_large = || kind.unwrap_or(Kind::Document).too_large();
                file = Some((name, read_capped(field, cap, too_large, kind).await?));
            }
            Some("kind" | "file") => {
                return Err(unprocessable("Field kind dan file hanya boleh satu."));
            }
            _ => {}
        }
    }
    let kind = kind.ok_or_else(kind_error)?;
    let (name, data) = file.ok_or_else(|| unprocessable("Field file wajib diisi."))?;
    if data.len() > kind.max_bytes() {
        return Err(kind.too_large());
    }
    Ok(Parts { kind, name, data })
}

fn kind_error() -> ApiError {
    unprocessable("Field kind wajib diisi dengan image atau document.")
}

/// The format `parts` really is, or the 422 explaining why not.
fn detect(parts: &Parts) -> Result<Format, ApiError> {
    if parts.data.is_empty() {
        return Err(unprocessable("Berkas tidak boleh kosong."));
    }
    match parts.kind {
        Kind::Image => {
            validate_webp(&parts.data).map_err(unprocessable)?;
            Ok(Format::Webp)
        }
        Kind::Document => {
            let format = extension(&parts.name)
                .and_then(|ext| Format::from_ext(&ext))
                .filter(|format| !format.is_image())
                .ok_or_else(|| {
                    unprocessable(
                        "Tipe dokumen tidak didukung. Gunakan pdf, doc, docx, xls, xlsx, atau txt.",
                    )
                })?;
            validate_document(format, &parts.data).map_err(unprocessable)?;
            Ok(format)
        }
    }
}

/// Writes `data` to `{root}/{yyyy}/{mm}/{uuid}.{ext}` through a temporary file in the same
/// directory, so readers never see a partial file. Returns the path relative to `root`.
async fn store(root: &Path, format: Format, data: &[u8]) -> io::Result<String> {
    let now = Utc::now();
    let (year, month) = (format!("{:04}", now.year()), format!("{:02}", now.month()));
    let dir = root.join(&year).join(&month);
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o755)
        .create(&dir)
        .await?;

    let id = Uuid::new_v4();
    let name = format!("{id}.{}", format.ext());
    let tmp = dir.join(format!(".{id}.tmp"));
    let result = async {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o644)
            .open(&tmp)
            .await?;
        file.write_all(data).await?;
        file.sync_all().await?;
        drop(file);
        fs::rename(&tmp, dir.join(&name)).await
    }
    .await;
    if let Err(err) = result {
        let _ = fs::remove_file(&tmp).await;
        return Err(err);
    }
    Ok(format!("{year}/{month}/{name}"))
}

/// `PUBLIC_BASE_URL`, else `http://{Host}` (development fallback).
fn base_url(config: &Config, headers: &HeaderMap) -> String {
    if !config.public_base_url.is_empty() {
        return config.public_base_url.clone();
    }
    let host = headers
        .get(header::HOST)
        .and_then(|host| host.to_str().ok())
        .filter(|host| {
            !host.is_empty()
                && host.len() <= 255
                && host.bytes().all(|b| {
                    b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b':' | b'[' | b']')
                })
        });
    match host {
        Some(host) => format!("http://{host}"),
        None => format!("http://localhost:{}", config.port),
    }
}

pub async fn upload(
    user: CurrentUser,
    State(state): State<AppState>,
    headers: HeaderMap,
    multipart: Result<Multipart, MultipartRejection>,
) -> ApiResult<Uploaded> {
    check_user_limit(&state, &user).await?;
    let multipart =
        multipart.map_err(|_| unprocessable("Permintaan harus berformat multipart/form-data."))?;
    let parts = read_parts(multipart).await?;
    let format = detect(&parts)?;

    let relative = store(&state.config.upload_dir, format, &parts.data)
        .await
        .map_err(|err| {
            tracing::error!(error = %err, "could not store upload");
            ApiError::internal(err)
        })?;
    let url = format!("{}/uploads/{relative}", base_url(&state.config, &headers));
    if url.len() >= MAX_URL_LEN {
        tracing::error!(len = url.len(), "upload URL exceeds the column limit");
        return Err(ApiError::server());
    }
    created(
        Uploaded {
            url,
            kind: parts.kind.as_str(),
            size: parts.data.len(),
            content_type: format.content_type(),
            name: parts.name,
        },
        "Berkas berhasil diunggah",
    )
}
