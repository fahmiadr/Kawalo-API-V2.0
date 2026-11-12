//! Face registration and biometric data management (`/faces`). Verification is not an endpoint
//! of its own: it happens inside `POST /attendance`, so a match result cannot be replayed.
//!
//! Embeddings and photos are encrypted at rest (AES-256-GCM, `FACE_ENCRYPTION_KEY`); photos live
//! under the private `FACE_PHOTO_DIR` and are only served to group admins, every view audited.

mod handlers;
pub(crate) mod repository;

use std::sync::{Arc, LazyLock};

use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::extract::multipart::{Field, MultipartError, MultipartRejection};
use axum::extract::{FromRequestParts, Multipart, Path};
use axum::http::request::Parts;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use redis::Script;

use crate::core::error::{ApiError, ErrorCode};
use crate::core::extract::{CurrentUser, ValidationErrors};
use crate::core::face::{FaceError, Ready};
use crate::core::state::AppState;

/// Upper bound per photo; the app is expected to send a downscaled JPEG well below this.
pub(crate) const PHOTO_MAX_BYTES: usize = 4 * 1024 * 1024;
pub(crate) const MAX_REGISTER_PHOTOS: usize = 5;
const FIELD_MAX_BYTES: usize = 256;
const ADMIN_ROLES: &[&str] = &["ADMIN", "SUPER_ADMIN"];

/// Mounted under `/api/v1`.
pub fn router() -> Router<AppState> {
    let register_limit = MAX_REGISTER_PHOTOS * PHOTO_MAX_BYTES + 64 * 1024;
    Router::new()
        .route(
            "/faces/register",
            post(handlers::register).layer(DefaultBodyLimit::max(register_limit)),
        )
        .route("/faces/status", get(handlers::status))
        .route("/faces/me", delete(handlers::delete_mine))
        .route("/faces/{email}", delete(handlers::delete_other))
        .route("/faces/{email}/photos", get(handlers::list_photos))
        .route("/faces/{email}/photos/{id}", get(handlers::photo))
}

/// The engine, or 503: there is no way to register or verify without a working model and key.
pub(crate) fn ready(state: &AppState) -> Result<Arc<Ready>, ApiError> {
    state.faces.ready().cloned().ok_or_else(|| {
        ApiError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            ErrorCode::FaceServiceUnavailable,
            "Fitur pengenalan wajah belum aktif di server. Hubungi admin.",
        )
    })
}

/// A photo-level failure as an API error; engine failures are logged and become 503.
pub(crate) fn face_error(err: &FaceError) -> ApiError {
    match err {
        FaceError::Engine(cause) => {
            tracing::error!("[Face] inference failed: {cause}");
            ApiError::new(
                StatusCode::SERVICE_UNAVAILABLE,
                ErrorCode::FaceServiceUnavailable,
                err.message(),
            )
        }
        FaceError::InvalidImage => unprocessable(err.message()),
        _ => ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            ErrorCode::FaceQualityLow,
            format!("{} Ambil ulang foto wajah.", err.message()),
        ),
    }
}

pub(crate) fn unprocessable(message: impl Into<String>) -> ApiError {
    ApiError::new(
        StatusCode::UNPROCESSABLE_ENTITY,
        ErrorCode::ValidationError,
        message,
    )
}

/// `include_left` lets deletion reach members who left; views never do.
/// Passes for `ADMIN`/`SUPER_ADMIN` by token role (never the `allow_admin` flag, which real
/// guards have), and only for targets in the caller's active group, `SUPER_ADMIN` included (biometric data is not cross-tenant).
pub(crate) async fn require_admin_over(
    state: &AppState,
    user: &CurrentUser,
    target_email: &str,
    include_left: bool,
) -> Result<(), ApiError> {
    user.require_role(ADMIN_ROLES).map_err(|_| {
        ApiError::forbidden().with_message("Hanya administrator yang dapat mengakses data wajah.")
    })?;
    let group_uid = user.require_group_uid()?;
    if repository::is_member(&state.db, group_uid, target_email, include_left).await? {
        Ok(())
    } else {
        Err(ApiError::not_found().with_message("Pengguna tidak ditemukan di grup ini."))
    }
}

/// A decrypted photo, never cached by intermediaries.
pub(crate) fn jpeg_response(bytes: Vec<u8>) -> Response {
    (
        [
            (header::CONTENT_TYPE, "image/jpeg"),
            (header::CACHE_CONTROL, "private, no-store"),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
        ],
        bytes,
    )
        .into_response()
}

/// Increment and set the expiry in one step so a crash cannot leave a counter without a TTL.
static BUMP: LazyLock<Script> = LazyLock::new(|| {
    Script::new(
        "local n = redis.call('INCR', KEYS[1]) \
         if n == 1 then redis.call('EXPIRE', KEYS[1], ARGV[1]) end \
         return {n, redis.call('TTL', KEYS[1])}",
    )
});

/// Fixed-window limit per key; the window starts at the first attempt.
pub(crate) async fn throttle(
    state: &AppState,
    key: &str,
    limit: i64,
    window_seconds: i64,
) -> Result<(), ApiError> {
    let mut redis = state.redis.clone();
    let (count, ttl): (i64, i64) = BUMP
        .key(key)
        .arg(window_seconds)
        .invoke_async(&mut redis)
        .await?;
    if count <= limit {
        return Ok(());
    }
    let wait = u64::try_from(ttl.max(1)).unwrap_or(1);
    Err(ApiError::rate_limit(Some(wait)).with_message(format!(
        "Terlalu banyak percobaan. Coba lagi dalam {wait} detik."
    )))
}

/// A `{email}` path segment.
#[derive(Debug)]
pub struct EmailParam(pub String);

impl<S: Send + Sync> FromRequestParts<S> for EmailParam {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let Path(params) = Path::<Vec<(String, String)>>::from_request_parts(parts, state)
            .await
            .map_err(|_| ApiError::invalid_input("/email: Expected required property"))?;
        let email = params
            .into_iter()
            .find(|(k, _)| k == "email")
            .map(|(_, v)| v.trim().to_owned())
            .unwrap_or_default();
        let mut errors = ValidationErrors::new();
        errors.length("email", &email, 3..=128);
        errors.email("email", &email);
        errors
            .into_result()
            .map_err(|errors| ApiError::invalid_input(errors.to_string()))?;
        Ok(Self(email))
    }
}

/// Multipart body with photo fields and short text fields.
#[derive(Debug, Default)]
pub(crate) struct PhotoForm {
    pub photos: Vec<Vec<u8>>,
    fields: Vec<(String, String)>,
}

impl PhotoForm {
    pub fn field(&self, name: &str) -> Option<&str> {
        self.fields
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.trim())
            .filter(|v| !v.is_empty())
    }
}

fn photo_too_large() -> ApiError {
    ApiError::new(
        StatusCode::PAYLOAD_TOO_LARGE,
        ErrorCode::BadRequest,
        format!(
            "Ukuran foto maksimal {} MB.",
            PHOTO_MAX_BYTES / (1024 * 1024)
        ),
    )
}

fn multipart_error(err: &MultipartError) -> ApiError {
    if err.status() == StatusCode::PAYLOAD_TOO_LARGE {
        return photo_too_large();
    }
    unprocessable("Permintaan multipart tidak valid.")
}

async fn read_capped(
    mut field: Field<'_>,
    max_bytes: usize,
    too_large: impl Fn() -> ApiError,
) -> Result<Vec<u8>, ApiError> {
    let mut data = Vec::new();
    while let Some(chunk) = field.chunk().await.map_err(|err| multipart_error(&err))? {
        if data.len() + chunk.len() > max_bytes {
            return Err(too_large());
        }
        data.extend_from_slice(&chunk);
    }
    Ok(data)
}

/// Reads up to `max_photos` `photo_field` parts (each at most [`PHOTO_MAX_BYTES`]) and every
/// other part as a text field of at most 256 bytes. A repeated text field is rejected.
pub(crate) async fn read_photo_form(
    multipart: Result<Multipart, MultipartRejection>,
    photo_field: &str,
    max_photos: usize,
) -> Result<PhotoForm, ApiError> {
    let mut multipart =
        multipart.map_err(|_| unprocessable("Permintaan harus berformat multipart/form-data."))?;
    let mut form = PhotoForm::default();
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|err| multipart_error(&err))?
    {
        let Some(name) = field.name().map(str::to_owned) else {
            continue;
        };
        if name == photo_field {
            if form.photos.len() >= max_photos {
                return Err(unprocessable(format!("Maksimal {max_photos} foto.")));
            }
            let data = read_capped(field, PHOTO_MAX_BYTES, photo_too_large).await?;
            if data.is_empty() {
                return Err(unprocessable("Foto tidak boleh kosong."));
            }
            form.photos.push(data);
        } else {
            if form.fields.iter().any(|(n, _)| *n == name) {
                return Err(unprocessable(format!("Field {name} hanya boleh satu.")));
            }
            let too_long = || unprocessable(format!("Field {name} terlalu panjang."));
            let raw = read_capped(field, FIELD_MAX_BYTES, too_long).await?;
            let text = String::from_utf8(raw)
                .map_err(|_| unprocessable(format!("Field {name} harus berupa teks.")))?;
            form.fields.push((name, text));
        }
    }
    Ok(form)
}
