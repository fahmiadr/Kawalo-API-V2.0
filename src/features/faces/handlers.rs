use axum::extract::multipart::MultipartRejection;
use axum::extract::{FromRequestParts, Multipart, Path, State};
use axum::http::StatusCode;
use axum::http::request::Parts;
use axum::response::Response;
use chrono::{SubsecRound, Utc};
use serde::Serialize;

use super::repository::{self, Audit, NewRegistration};
use super::{
    EmailParam, MAX_REGISTER_PHOTOS, face_error, jpeg_response, read_photo_form, ready,
    require_admin_over, throttle, unprocessable,
};
use crate::core::clean::iso_string;
use crate::core::error::{ApiError, ErrorCode};
use crate::core::extract::CurrentUser;
use crate::core::face::{
    FaceError, cosine, embedding_from_bytes, embedding_to_bytes, mean_embedding, remove_photo,
};
use crate::core::ids::generate_uuid_v4;
use crate::core::response::{ApiResult, created, ok};
use crate::core::state::AppState;

const REGISTER_LIMIT: i64 = 10;
const REGISTER_WINDOW_SECONDS: i64 = 600;

#[derive(Debug, Serialize)]
pub struct FaceStatus {
    /// False when the server has no working model or key; the app should not start registration.
    available: bool,
    registered: bool,
    registered_at: Option<String>,
    photo_count: i32,
    /// Registered with another model than the one loaded now: verification will refuse until
    /// the user deletes and registers again.
    needs_reregistration: bool,
}

#[derive(Debug, Serialize)]
pub struct Rejected {
    /// 1-based position of the photo in the request.
    index: usize,
    reason: &'static str,
}

#[derive(Debug, Serialize)]
pub struct Registered {
    registered_at: String,
    photo_count: usize,
    rejected: Vec<Rejected>,
}

#[derive(Debug, Serialize)]
pub struct Deleted {
    deleted: bool,
}

#[derive(Debug, Serialize)]
pub struct PhotoItem {
    id: String,
    created_at: String,
    /// Authenticated binary download (admin only), relative to the API origin.
    url: String,
}

pub async fn status(user: CurrentUser, State(state): State<AppState>) -> ApiResult<FaceStatus> {
    let row = repository::find(&state.db, &user.email).await?;
    let current = state.faces.ready().map(|r| r.model_version().to_owned());
    ok(
        FaceStatus {
            available: current.is_some(),
            registered: row.is_some(),
            registered_at: row.as_ref().map(|r| iso_string(r.created_at)),
            photo_count: row.as_ref().map_or(0, |r| r.photo_count),
            needs_reregistration: match (&row, &current) {
                (Some(row), Some(current)) => row.model_version != *current,
                _ => false,
            },
        },
        "Status data wajah berhasil diambil",
    )
}

/// Self-registration only. Photos failing the quality gate are dropped; the rest must reach the
/// configured minimum, show the same person, and not match another member of the group.
pub async fn register(
    user: CurrentUser,
    State(state): State<AppState>,
    multipart: Result<Multipart, MultipartRejection>,
) -> ApiResult<Registered> {
    let group_uid = user.require_group_uid()?;
    let ready = ready(&state)?;
    throttle(
        &state,
        &format!("face:register:{}", user.sub),
        REGISTER_LIMIT,
        REGISTER_WINDOW_SECONDS,
    )
    .await?;
    if repository::find(&state.db, &user.email).await?.is_some() {
        return Err(already_registered());
    }
    let form = read_photo_form(multipart, "photos", MAX_REGISTER_PHOTOS).await?;
    if form.photos.is_empty() {
        return Err(unprocessable("Field photos wajib diisi."));
    }
    let submitted = form.photos.len();

    let mut accepted = Vec::new();
    let mut rejected = Vec::new();
    for (i, photo) in form.photos.into_iter().enumerate() {
        match ready.embed_photo(photo).await {
            Ok(sample) => accepted.push(sample),
            Err(err @ FaceError::Engine(_)) => return Err(face_error(&err)),
            Err(err) => rejected.push(Rejected {
                index: i + 1,
                reason: err.message(),
            }),
        }
    }
    let config = &state.config.faces;
    let minimum = config.register_min_photos.max(1);
    if accepted.len() < minimum {
        let reasons: Vec<String> = rejected
            .iter()
            .map(|r| format!("Foto {}: {}", r.index, r.reason))
            .collect();
        return Err(quality_low(format!(
            "Hanya {} dari {submitted} foto yang memenuhi syarat (minimal {minimum}). {}",
            accepted.len(),
            reasons.join(" ")
        )));
    }

    let samples: Vec<Vec<f32>> = accepted.iter().map(|(_, e)| e.clone()).collect();
    let mean = mean_embedding(&samples)
        .ok_or_else(|| ApiError::internal("embeddings of unequal length"))?;
    // Pairwise, not against the mean: two different people still sit about 0.7 from their mean.
    let mixed = samples.iter().enumerate().any(|(i, a)| {
        samples[i + 1..]
            .iter()
            .any(|b| cosine(a, b) < config.match_threshold)
    });
    if mixed {
        return Err(quality_low(
            "Foto-foto berisi wajah yang berbeda. Ulangi pendaftaran dengan wajah Anda sendiri.",
        ));
    }

    let version = ready.model_version().to_owned();
    for other in repository::group_embeddings(&state.db, group_uid, &user.email).await? {
        if other.model_version != version {
            continue;
        }
        let Some(vector) = ready
            .open(&other.embedding, &other.email)
            .and_then(|plain| embedding_from_bytes(&plain))
        else {
            tracing::warn!("[Face] a stored embedding of the group could not be decrypted");
            continue;
        };
        if cosine(&mean, &vector) >= config.duplicate_threshold {
            repository::audit_after(
                &state.db,
                Audit {
                    action: repository::AUDIT_DUPLICATE_BLOCKED,
                    actor: &user.email,
                    target: &user.email,
                    group_uid: Some(group_uid),
                    reference: Some(&other.email),
                },
            )
            .await;
            return Err(ApiError::new(
                StatusCode::CONFLICT,
                ErrorCode::FaceDuplicate,
                "Wajah ini sudah terdaftar atas nama pengguna lain di grup ini. Hubungi admin.",
            ));
        }
    }

    let dir = &config.photo_dir;
    let mut photos = Vec::new();
    for (jpeg, _) in &accepted {
        match ready.store_photo(dir, jpeg).await {
            Ok(path) => photos.push((generate_uuid_v4(), path)),
            Err(err) => {
                remove_all(dir, &photos).await;
                tracing::error!(error = %err, "[Face] could not store a registration photo");
                return Err(ApiError::internal(err));
            }
        }
    }
    let sealed = ready
        .seal(&embedding_to_bytes(&mean), &user.email)
        .map_err(|_| ApiError::server())?;
    let now = Utc::now().trunc_subsecs(0);
    let inserted = repository::insert(
        &state.db,
        &NewRegistration {
            email: &user.email,
            embedding: &sealed,
            model_version: &version,
            photos: &photos,
            now,
        },
    )
    .await;
    if let Err(err) = inserted {
        remove_all(dir, &photos).await;
        return Err(match err {
            sqlx::Error::Database(e) if e.is_unique_violation() => already_registered(),
            other => other.into(),
        });
    }
    repository::audit_after(
        &state.db,
        Audit {
            action: repository::AUDIT_REGISTER,
            actor: &user.email,
            target: &user.email,
            group_uid: Some(group_uid),
            reference: Some(&version),
        },
    )
    .await;
    created(
        Registered {
            registered_at: iso_string(now),
            photo_count: photos.len(),
            rejected,
        },
        "Wajah berhasil didaftarkan",
    )
}

/// Works without a loaded model: deletion must always be possible.
pub async fn delete_mine(user: CurrentUser, State(state): State<AppState>) -> ApiResult<Deleted> {
    let deleted = erase(&state, &user.email).await?;
    if deleted {
        repository::audit_after(
            &state.db,
            Audit {
                action: repository::AUDIT_DELETE_SELF,
                actor: &user.email,
                target: &user.email,
                group_uid: user.group_uid.as_deref(),
                reference: None,
            },
        )
        .await;
    }
    ok(Deleted { deleted }, deleted_message(deleted))
}

pub async fn delete_other(
    EmailParam(email): EmailParam,
    user: CurrentUser,
    State(state): State<AppState>,
) -> ApiResult<Deleted> {
    require_admin_over(&state, &user, &email, true).await?;
    let deleted = erase(&state, &email).await?;
    if deleted {
        repository::audit_after(
            &state.db,
            Audit {
                action: repository::AUDIT_DELETE_BY_ADMIN,
                actor: &user.email,
                target: &email,
                group_uid: user.group_uid.as_deref(),
                reference: None,
            },
        )
        .await;
    }
    ok(Deleted { deleted }, deleted_message(deleted))
}

pub async fn list_photos(
    EmailParam(email): EmailParam,
    user: CurrentUser,
    State(state): State<AppState>,
) -> ApiResult<Vec<PhotoItem>> {
    require_admin_over(&state, &user, &email, false).await?;
    let items = repository::photos(&state.db, &email)
        .await?
        .into_iter()
        .map(|row| PhotoItem {
            url: format!(
                "/api/v1/faces/{}/photos/{}",
                urlencode(&email),
                urlencode(&row.id)
            ),
            created_at: iso_string(row.created_at),
            id: row.id,
        })
        .collect();
    ok(items, "Foto wajah berhasil diambil")
}

/// The `{id}` of a registration photo.
#[derive(Debug)]
pub struct PhotoId(String);

impl<S: Send + Sync> FromRequestParts<S> for PhotoId {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let Path(params) = Path::<Vec<(String, String)>>::from_request_parts(parts, state)
            .await
            .map_err(|_| ApiError::invalid_input("/id: Expected required property"))?;
        params
            .into_iter()
            .find(|(k, _)| k == "id")
            .map(|(_, v)| v)
            .filter(|v| (1..=64).contains(&v.len()))
            .map(Self)
            .ok_or_else(|| ApiError::invalid_input("/id: Expected string length 1..64"))
    }
}

/// Decrypted registration photo; refused when the view cannot be audited.
pub async fn photo(
    EmailParam(email): EmailParam,
    PhotoId(id): PhotoId,
    user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Response, ApiError> {
    require_admin_over(&state, &user, &email, false).await?;
    let ready = ready(&state)?;
    let not_found = || ApiError::not_found().with_message("Foto wajah tidak ditemukan.");
    let path = repository::photo(&state.db, &email, &id)
        .await?
        .ok_or_else(not_found)?;
    let bytes = ready
        .read_photo(&state.config.faces.photo_dir, &path)
        .await
        .ok_or_else(not_found)?;
    repository::audit(
        &state.db,
        Audit {
            action: repository::AUDIT_VIEW_REGISTER_PHOTO,
            actor: &user.email,
            target: &email,
            group_uid: user.group_uid.as_deref(),
            reference: Some(&id),
        },
    )
    .await?;
    Ok(jpeg_response(bytes))
}

async fn erase(state: &AppState, email: &str) -> Result<bool, ApiError> {
    let (deleted, paths) = repository::delete(&state.db, email).await?;
    for path in &paths {
        remove_photo(&state.config.faces.photo_dir, path).await;
    }
    Ok(deleted)
}

async fn remove_all(dir: &std::path::Path, photos: &[(String, String)]) {
    for (_, path) in photos {
        remove_photo(dir, path).await;
    }
}

fn deleted_message(deleted: bool) -> &'static str {
    if deleted {
        "Data wajah berhasil dihapus"
    } else {
        "Tidak ada data wajah yang tersimpan"
    }
}

fn already_registered() -> ApiError {
    ApiError::new(
        StatusCode::CONFLICT,
        ErrorCode::FaceAlreadyRegistered,
        "Wajah sudah terdaftar. Hapus data wajah terlebih dahulu untuk mendaftar ulang.",
    )
}

fn quality_low(message: impl Into<String>) -> ApiError {
    ApiError::new(
        StatusCode::UNPROCESSABLE_ENTITY,
        ErrorCode::FaceQualityLow,
        message,
    )
}

fn urlencode(raw: &str) -> String {
    form_urlencoded::byte_serialize(raw.as_bytes()).collect()
}
