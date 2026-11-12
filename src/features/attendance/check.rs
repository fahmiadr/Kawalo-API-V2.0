//! `POST /attendance/nonce`, `POST /attendance` (check-in/out with face verification) and the
//! admin-only `GET /attendance/{id}/photo`.

use std::sync::LazyLock;

use axum::extract::multipart::MultipartRejection;
use axum::extract::{FromRequestParts, Multipart, Path, State};
use axum::http::StatusCode;
use axum::http::request::Parts;
use axum::response::Response;
use chrono::{Local, SubsecRound, Utc};
use redis::{AsyncCommands, Script};
use serde::Serialize;

use super::check_repository::{self as repository, NewAbsen, Scheduled};
use super::entity::Category;
use super::manual;
use super::rules::{self, Geofence, Kind, Refusal, Status};
use super::setting;
use crate::core::clean::{clean_string, iso_string};
use crate::core::error::{ApiError, ErrorCode};
use crate::core::extract::CurrentUser;
use crate::core::face::{cosine, embedding_from_bytes, remove_photo};
use crate::core::ids::generate_uuid_v4;
use crate::core::response::{ApiResult, created};
use crate::core::state::AppState;
use crate::features::faces::repository::{self as faces_repository, Audit};
use crate::features::faces::{
    face_error, jpeg_response, read_photo_form, ready, throttle, unprocessable,
};

const NONCE_LIMIT: i64 = 12;
const NONCE_WINDOW_SECONDS: i64 = 60;
/// `tkabsen.ALocation` is VARCHAR(64).
const LOCATION_MAX_CHARS: usize = 64;

/// Get-and-delete in one step, so a nonce is accepted once even under concurrent requests
/// (`GETDEL` needs Redis 6.2; the project supports 6.0).
static TAKE: LazyLock<Script> = LazyLock::new(|| {
    Script::new(
        "local v = redis.call('GET', KEYS[1]) \
         if v then redis.call('DEL', KEYS[1]) end \
         return v",
    )
});

fn nonce_key(nonce: &str) -> String {
    format!("attendance:nonce:{nonce}")
}

#[derive(Debug, Serialize)]
pub struct IssuedNonce {
    nonce: String,
    expires_in: u64,
    failed_attempts: i64,
    manual_request_available: bool,
}

/// A fresh single-use nonce bound to the caller. The device must request one per attempt.
pub async fn nonce(user: CurrentUser, State(state): State<AppState>) -> ApiResult<IssuedNonce> {
    user.require_group_uid()?;
    throttle(
        &state,
        &format!("attendance:nonce-rate:{}", user.sub),
        NONCE_LIMIT,
        NONCE_WINDOW_SECONDS,
    )
    .await?;
    let ttl = state.config.attendance.nonce_ttl_seconds.max(1);
    let nonce = generate_uuid_v4();
    let mut redis = state.redis.clone();
    let () = redis.set_ex(nonce_key(&nonce), &user.sub, ttl).await?;
    let failed_attempts = manual::failures(&state, &user.email).await?;
    created(
        IssuedNonce {
            nonce,
            expires_in: ttl,
            failed_attempts,
            manual_request_available: failed_attempts >= manual::MAX_FACE_ATTEMPTS,
        },
        "Nonce absensi berhasil dibuat",
    )
}

async fn take_nonce(state: &AppState, user: &CurrentUser, nonce: &str) -> Result<(), ApiError> {
    let mut redis = state.redis.clone();
    let owner: Option<String> = TAKE.key(nonce_key(nonce)).invoke_async(&mut redis).await?;
    if owner.as_deref() == Some(user.sub.as_str()) {
        Ok(())
    } else {
        Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            ErrorCode::AttendanceNonceInvalid,
            "Sesi absensi tidak valid atau kedaluwarsa. Silakan ulangi absensi.",
        ))
    }
}

#[derive(Debug, Serialize)]
pub struct CheckResult {
    id: String,
    attendance_type: &'static str,
    category: Category,
    attendance_dt: String,
    /// `on_time` = green; `late` (check-in) and `early_leave` (check-out) = red.
    status: Status,
    late_minutes: i32,
    early_leave_minutes: i32,
    schedule_date: String,
    shift_uid: String,
    shift_name: String,
    shift_time_window: String,
    location_name: Option<String>,
    /// `null` when the shift has no location or radius (no geofence).
    distance_meters: Option<i64>,
    radius_meters: Option<i32>,
}

/// Whole percent of a cosine score; negative scores (opposite vectors) read as 0.
pub(super) fn percent(score: f32) -> i32 {
    (score.clamp(0.0, 1.0) * 100.0).round() as i32
}

struct Input {
    photo: Vec<u8>,
    latitude: f64,
    longitude: f64,
    nonce: String,
    address: Option<String>,
}

fn coordinate(raw: Option<&str>, name: &str, limit: f64) -> Result<f64, ApiError> {
    raw.and_then(|v| v.parse::<f64>().ok())
        .filter(|v| v.is_finite() && v.abs() <= limit)
        .ok_or_else(|| unprocessable(format!("Field {name} wajib berupa koordinat yang valid.")))
}

async fn read_input(multipart: Result<Multipart, MultipartRejection>) -> Result<Input, ApiError> {
    let mut form = read_photo_form(multipart, "photo", 1).await?;
    let latitude = coordinate(form.field("latitude"), "latitude", 90.0)?;
    let longitude = coordinate(form.field("longitude"), "longitude", 180.0)?;
    let nonce = form
        .field("nonce")
        .filter(|n| n.len() <= 64)
        .map(str::to_owned)
        .ok_or_else(|| unprocessable("Field nonce wajib diisi."))?;
    let address = clean_string(form.field("address"));
    let photo = form
        .photos
        .pop()
        .ok_or_else(|| unprocessable("Field photo wajib diisi."))?;
    Ok(Input {
        photo,
        latitude,
        longitude,
        nonce,
        address,
    })
}

/// Cheap checks first (nonce, schedule, radius), the face last: inference is the expensive part
/// and its result is never returned on its own.
pub async fn check(
    user: CurrentUser,
    State(state): State<AppState>,
    multipart: Result<Multipart, MultipartRejection>,
) -> ApiResult<CheckResult> {
    let group_uid = user.require_group_uid()?.to_owned();
    let ready = ready(&state)?;
    let input = read_input(multipart).await?;
    take_nonce(&state, &user, &input.nonce).await?;

    let now = Local::now().trunc_subsecs(0);
    let now_local = now.naive_local();
    let schedules =
        repository::around(&state.db, &group_uid, &user.email, now_local.date()).await?;
    let slots: Vec<_> = schedules.iter().map(|s| s.slot.clone()).collect();
    let (index, kind) = rules::decide(&slots, now_local).map_err(refusal)?;
    let scheduled = &schedules[index];

    let geofence = rules::geofence(
        &scheduled.loc_lat,
        &scheduled.loc_long,
        scheduled.loc_radius,
        (input.latitude, input.longitude),
    );
    if let Geofence::Outside {
        distance_m,
        radius_m,
    } = geofence
    {
        return Err(out_of_radius(distance_m, radius_m));
    }

    let registered = faces_repository::find(&state.db, &user.email)
        .await?
        .ok_or_else(|| {
            not_registered("Wajah Anda belum terdaftar. Daftarkan wajah terlebih dahulu.")
        })?;
    if registered.model_version != ready.model_version() {
        return Err(not_registered(
            "Data wajah Anda perlu didaftarkan ulang. Hapus data wajah lalu daftar kembali.",
        ));
    }
    let reference = ready
        .open(&registered.embedding, &registered.email)
        .and_then(|plain| embedding_from_bytes(&plain))
        .ok_or_else(|| {
            tracing::error!("[Face] stored embedding could not be decrypted");
            not_registered("Data wajah Anda tidak dapat dibaca. Daftarkan ulang wajah Anda.")
        })?;
    let (jpeg, probe) = ready
        .embed_photo(input.photo)
        .await
        .map_err(|err| face_error(&err))?;
    let score = cosine(&reference, &probe);
    let threshold = setting::group_threshold(&state, &group_uid)
        .await?
        .unwrap_or(state.config.faces.match_threshold);
    if score < threshold {
        tracing::warn!(similarity = score, threshold, "[Face] check-in mismatch");
        let failed = manual::count_failure(&state, &user.email).await?;
        let progress = manual::progress(failed);
        return Err(ApiError::new(
            StatusCode::FORBIDDEN,
            ErrorCode::FaceNotMatch,
            "Wajah tidak cocok dengan data terdaftar. Coba lagi dengan pencahayaan yang cukup.",
        )
        .with_details(progress.clone())
        .with_data(progress));
    }

    let tolerance = &state.config.attendance;
    let (late, early) = rules::lateness(
        kind,
        &scheduled.slot,
        now_local,
        tolerance.late_tolerance_minutes,
        tolerance.early_leave_tolerance_minutes,
    );

    let photo_dir = &state.config.faces.photo_dir;
    let photo_path = ready.store_photo(photo_dir, &jpeg).await.map_err(|err| {
        tracing::error!(error = %err, "[Face] could not store an attendance photo");
        ApiError::internal(err)
    })?;
    let location = location_text(input.address.as_deref(), &scheduled.loc_name);
    let at = now.with_timezone(&Utc);
    let absen = NewAbsen {
        email: &user.email,
        name: user.name.as_deref().unwrap_or("N/A"),
        at,
        kind,
        lat: format_coordinate(input.latitude),
        long: format_coordinate(input.longitude),
        late_mins: late,
        early_mins: early,
        location: &location,
        shift_uid: &scheduled.shift_uid,
        group_uid: &group_uid,
        shift_start: scheduled.slot.start.format("%H:%M:%S").to_string(),
        face_verified: true,
        face_score: Some(score),
        photo_path: Some(&photo_path),
        clock: now_local.format("%H:%M:%S").to_string(),
    };
    let id = match repository::record(&state.db, scheduled.schedule_id, &absen).await {
        Ok(Some(id)) => id,
        Ok(None) => {
            remove_photo(photo_dir, &photo_path).await;
            return Err(refusal(Refusal::Completed));
        }
        Err(err) => {
            remove_photo(photo_dir, &photo_path).await;
            return Err(err.into());
        }
    };

    manual::clear_failures(&state, &user.email, now_local.date()).await;

    let (distance_meters, radius_meters) = match geofence {
        Geofence::Inside {
            distance_m,
            radius_m,
        } => (Some(distance_m.round() as i64), Some(radius_m)),
        _ => (None, None),
    };
    let message = match kind {
        Kind::In => "Absen masuk berhasil",
        Kind::Out => "Absen pulang berhasil",
    };
    created(
        CheckResult {
            id: id.to_string(),
            attendance_type: kind.absen_type(),
            category: Category::of(Some(kind.absen_type())),
            attendance_dt: iso_string(at),
            status: Status::of(kind, late, early),
            late_minutes: late,
            early_leave_minutes: early,
            schedule_date: scheduled.slot.date.format("%Y-%m-%d").to_string(),
            shift_uid: scheduled.shift_uid.clone(),
            shift_name: scheduled.shift_name.clone(),
            shift_time_window: time_window(scheduled),
            location_name: clean_string(Some(&location)),
            distance_meters,
            radius_meters,
        },
        message,
    )
}

pub(super) fn out_of_radius(distance_m: f64, radius_m: i32) -> ApiError {
    ApiError::new(
        StatusCode::FORBIDDEN,
        ErrorCode::AttendanceOutOfRadius,
        format!(
            "Anda berada {} m dari lokasi absensi (batas {radius_m} m). Mendekatlah ke lokasi lalu coba lagi.",
            distance_m.round() as i64
        ),
    )
}

pub(super) fn refusal(refusal: Refusal) -> ApiError {
    match refusal {
        Refusal::NoSchedule => ApiError::new(
            StatusCode::NOT_FOUND,
            ErrorCode::AttendanceNoSchedule,
            "Tidak ada jadwal shift Anda untuk hari ini.",
        ),
        Refusal::Completed => ApiError::new(
            StatusCode::CONFLICT,
            ErrorCode::AttendanceAlreadyCompleted,
            "Anda sudah absen masuk dan pulang untuk shift hari ini.",
        ),
    }
}

fn not_registered(message: &str) -> ApiError {
    ApiError::new(StatusCode::CONFLICT, ErrorCode::FaceNotRegistered, message)
}

pub(super) fn time_window(scheduled: &Scheduled) -> String {
    format!(
        "{} - {}",
        scheduled.slot.start.format("%H:%M"),
        scheduled.slot.stop.format("%H:%M")
    )
}

/// The device address when given, else the shift location name; cut to the column width.
pub(super) fn location_text(address: Option<&str>, shift_location: &str) -> String {
    let text = address.unwrap_or(shift_location).trim();
    let text = if text.is_empty() { "N/A" } else { text };
    text.chars().take(LOCATION_MAX_CHARS).collect()
}

/// Seven decimals (about 1 cm) fit `ALat`/`ALong` (VARCHAR(32)).
pub(super) fn format_coordinate(value: f64) -> String {
    format!("{value:.7}")
}

/// The `{id}` of an attendance row.
#[derive(Debug)]
pub struct AbsenId(pub i32);

impl<S: Send + Sync> FromRequestParts<S> for AbsenId {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let Path(raw) = Path::<String>::from_request_parts(parts, state)
            .await
            .map_err(|_| ApiError::invalid_input("/id: Expected required property"))?;
        raw.parse::<i32>()
            .ok()
            .filter(|id| *id > 0)
            .map(Self)
            .ok_or_else(|| ApiError::invalid_input("/id: Expected string to match '^[0-9]+$'"))
    }
}

/// The face photo of a check-in/out: admins of the row's group only (not cross-tenant, `SUPER_ADMIN`
/// included), refused when the view cannot be audited.
pub async fn photo(
    AbsenId(id): AbsenId,
    user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Response, ApiError> {
    user.require_role(&["ADMIN", "SUPER_ADMIN"]).map_err(|_| {
        ApiError::forbidden().with_message("Hanya administrator yang dapat melihat foto absensi.")
    })?;
    let not_found = || ApiError::not_found().with_message("Foto absensi tidak ditemukan.");
    let row = repository::photo_ref(&state.db, id)
        .await?
        .ok_or_else(not_found)?;
    let group_uid = user.require_group_uid()?;
    if row.group_uid.as_deref() != Some(group_uid) {
        return Err(not_found());
    }
    let path = clean_string(row.photo_path.as_deref()).ok_or_else(not_found)?;
    // Like registration photos: only while the person is a current member of the group.
    let owner = row.email.as_deref().unwrap_or_default();
    if !faces_repository::is_member(&state.db, group_uid, owner, false).await? {
        return Err(not_found());
    }
    let ready = ready(&state)?;
    let bytes = ready
        .read_photo(&state.config.faces.photo_dir, &path)
        .await
        .ok_or_else(not_found)?;
    let reference = id.to_string();
    faces_repository::audit(
        &state.db,
        Audit {
            action: faces_repository::AUDIT_VIEW_ATTENDANCE_PHOTO,
            actor: &user.email,
            target: row.email.as_deref().unwrap_or("N/A"),
            group_uid: row.group_uid.as_deref(),
            reference: Some(&reference),
        },
    )
    .await?;
    Ok(jpeg_response(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percent_is_a_whole_number_between_0_and_100() {
        assert_eq!(percent(0.874), 87);
        assert_eq!(percent(0.5), 50);
        assert_eq!(percent(-0.3), 0);
        assert_eq!(percent(1.2), 100);
    }

    #[test]
    fn location_falls_back_and_fits_the_column() {
        assert_eq!(location_text(Some("Jl. Sudirman"), "Lobby"), "Jl. Sudirman");
        assert_eq!(location_text(None, "Lobby"), "Lobby");
        assert_eq!(location_text(None, " "), "N/A");
        assert_eq!(
            location_text(Some(&"é".repeat(100)), "x").chars().count(),
            64
        );
        assert_eq!(format_coordinate(-6.2), "-6.2000000");
    }
}
