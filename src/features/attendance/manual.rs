//! Manual attendance: the escape hatch when the face check keeps failing or the camera is
//! unavailable. A guard files a request (`POST /attendance/manual-requests`), an admin of the
//! group approves or rejects it. Approval writes the same `tkabsen` row as `POST /attendance`,
//! stamped with the request time, but marked unverified (`AFaceVerified = 0`) and without photo.

use axum::extract::State;
use axum::http::StatusCode;
use chrono::{DateTime, Local, NaiveDate, SubsecRound, Utc};
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::check::{self, AbsenId};
use super::check_repository::{self as schedules, NewAbsen};
use super::entity::Category;
use super::manual_repository::{self as repository, Decision, NewRequest, Row};
use super::rules::{self, Geofence};
use crate::core::clean::{clean_string, iso_string};
use crate::core::error::{ApiError, ErrorCode};
use crate::core::extract::schema::{Body, Field, Kind as FieldKind, Schema};
use crate::core::extract::{
    AuthedJson, CurrentUser, Numeric, Validate, ValidatedQuery, ValidationErrors,
};
use crate::core::pagination::{PaginationParams, resolve_pagination};
use crate::core::response::{ApiResult, created, ok};
use crate::core::state::AppState;

/// Face mismatches in one day after which `face_failed` requests are allowed.
pub const MAX_FACE_ATTEMPTS: i64 = 3;
const NOTE_MAX: usize = 200;
const ADMIN_ROLES: [&str; 2] = ["ADMIN", "SUPER_ADMIN"];

const REASON_FACE_FAILED: &str = "face_failed";
const REASON_CAMERA: &str = "camera_unavailable";
const REASONS: [&str; 2] = [REASON_FACE_FAILED, REASON_CAMERA];
const STATUS_PENDING: &str = "PENDING";
const STATUS_APPROVED: &str = "APPROVED";
const STATUS_REJECTED: &str = "REJECTED";
const STATUSES: [&str; 3] = [STATUS_PENDING, STATUS_APPROVED, STATUS_REJECTED];
const ACTIONS: [&str; 2] = ["approve", "reject"];

fn fail_key(email: &str, day: NaiveDate) -> String {
    format!("attendance:face-fail:{email}:{}", day.format("%Y%m%d"))
}

/// Unix time of the next local midnight, when the day's counter expires.
fn end_of_day(now: DateTime<Local>) -> i64 {
    now.date_naive()
        .succ_opt()
        .and_then(|day| day.and_hms_opt(0, 0, 0))
        .and_then(|midnight| midnight.and_local_timezone(Local).earliest())
        .map_or(now.timestamp() + 86_400, |t| t.timestamp())
}

/// Face mismatches of the user today.
pub async fn failures(state: &AppState, email: &str) -> Result<i64, ApiError> {
    let mut redis = state.redis.clone();
    let count: Option<i64> = redis
        .get(fail_key(email, Local::now().date_naive()))
        .await?;
    Ok(count.unwrap_or(0))
}

/// Counts one mismatch and returns the new total. Counter and expiry are set in one transaction
/// so a crash cannot leave a counter that never expires.
pub async fn count_failure(state: &AppState, email: &str) -> Result<i64, ApiError> {
    let now = Local::now();
    let key = fail_key(email, now.date_naive());
    let mut redis = state.redis.clone();
    let (count,): (i64,) = redis::pipe()
        .atomic()
        .incr(&key, 1)
        .expire_at(&key, end_of_day(now))
        .ignore()
        .query_async(&mut redis)
        .await?;
    Ok(count)
}

/// A success or an approved request starts the next check (e.g. the check-out) from zero.
pub async fn clear_failures(state: &AppState, email: &str, day: NaiveDate) {
    let mut redis = state.redis.clone();
    let result: redis::RedisResult<()> = redis.del(fail_key(email, day)).await;
    if let Err(err) = result {
        tracing::warn!(error = %err, "[Attendance] could not reset the face failure counter");
    }
}

/// Sent with `FACE_NOT_MATCH` and `ATTENDANCE_MANUAL_UNAVAILABLE` so the app knows whether to
/// offer "Lapor ke admin".
pub fn progress(failed: i64) -> Value {
    json!({
        "failed_attempts": failed,
        "max_attempts": MAX_FACE_ATTEMPTS,
        "manual_request_available": failed >= MAX_FACE_ATTEMPTS,
    })
}

#[derive(Debug, Deserialize)]
pub struct RequestBody {
    reason: String,
    note: Option<String>,
    latitude: f64,
    longitude: f64,
}

impl Body for RequestBody {
    const SCHEMA: Schema = Schema::new(&[
        Field::required("reason", FieldKind::OneOf(&REASONS)),
        Field::nullable("note", FieldKind::text(0, NOTE_MAX)),
        Field::required("latitude", FieldKind::number(-90.0, 90.0)),
        Field::required("longitude", FieldKind::number(-180.0, 180.0)),
    ]);
}

#[derive(Debug, Deserialize)]
pub struct DecisionBody {
    action: String,
    note: Option<String>,
}

impl Body for DecisionBody {
    const SCHEMA: Schema = Schema::new(&[
        Field::required("action", FieldKind::OneOf(&ACTIONS)),
        Field::nullable("note", FieldKind::text(0, NOTE_MAX)),
    ]);
}

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    status: Option<String>,
    limit: Option<Numeric>,
    offset: Option<Numeric>,
}

impl Validate for ListQuery {
    fn validate(&self) -> Result<(), ValidationErrors> {
        let mut errors = ValidationErrors::new();
        if let Some(status) = &self.status
            && !STATUSES.contains(&status.as_str())
        {
            errors.push("status", "status harus PENDING, APPROVED, atau REJECTED");
        }
        errors.numeric(
            "limit",
            self.limit,
            0.0..=f64::from(crate::core::constants::MAX_PAGE_SIZE),
        );
        errors.numeric("offset", self.offset, 0.0..);
        errors.into_result()
    }
}

#[derive(Debug, Serialize)]
pub struct ManualItem {
    id: String,
    status: String,
    reason: String,
    note: Option<String>,
    user_email: String,
    user_name: String,
    attendance_type: String,
    category: Category,
    requested_at: String,
    latitude: f64,
    longitude: f64,
    distance_meters: Option<i32>,
    radius_meters: Option<i32>,
    shift_uid: String,
    shift_name: String,
    shift_time_window: String,
    location_name: Option<String>,
    failed_attempts: i32,
    decided_by: Option<String>,
    decided_at: Option<String>,
    decision_note: Option<String>,
    /// The `tkabsen` row created by an approval.
    attendance_id: Option<String>,
}

fn to_item(row: Row) -> ManualItem {
    ManualItem {
        id: row.id.to_string(),
        category: Category::of(Some(&row.absen_type)),
        attendance_type: row.absen_type,
        status: row.status,
        reason: row.reason,
        note: row.note,
        user_email: row.user_email,
        user_name: row.user_name,
        requested_at: iso_string(row.requested_at),
        latitude: row.latitude,
        longitude: row.longitude,
        distance_meters: row.distance_m,
        radius_meters: row.radius_m,
        shift_uid: row.shift_uid,
        shift_name: row.shift_name,
        shift_time_window: row.shift_window,
        location_name: clean_string(Some(&row.loc_name)),
        failed_attempts: row.failed_attempts,
        decided_by: row.decided_by,
        decided_at: row.decided_at.map(iso_string),
        decision_note: row.decision_note,
        attendance_id: row.absen_id.map(|id| id.to_string()),
    }
}

fn conflict(message: &str) -> ApiError {
    ApiError::new(StatusCode::CONFLICT, ErrorCode::ConflictError, message)
}

fn not_found() -> ApiError {
    ApiError::not_found().with_message("Permintaan absen manual tidak ditemukan.")
}

/// Same checks as `POST /attendance` except the face and nonce: schedule, radius, not finished.
pub async fn create(
    State(state): State<AppState>,
    AuthedJson(auth, body): AuthedJson<RequestBody>,
) -> ApiResult<ManualItem> {
    let user = auth.require_auth()?;
    let group_uid = user.require_group_uid()?.to_owned();
    let failed = failures(&state, &user.email).await?;
    if body.reason == REASON_FACE_FAILED && failed < MAX_FACE_ATTEMPTS {
        return Err(ApiError::new(
            StatusCode::FORBIDDEN,
            ErrorCode::AttendanceManualUnavailable,
            format!(
                "Laporan ke admin baru bisa dibuat setelah {MAX_FACE_ATTEMPTS} kali verifikasi wajah gagal."
            ),
        )
        .with_data(progress(failed)));
    }

    let now = Local::now().trunc_subsecs(0);
    let now_local = now.naive_local();
    let found = schedules::around(&state.db, &group_uid, &user.email, now_local.date()).await?;
    let slots: Vec<_> = found.iter().map(|s| s.slot.clone()).collect();
    let (index, kind) = rules::decide(&slots, now_local).map_err(check::refusal)?;
    let scheduled = &found[index];
    let (distance_m, radius_m) = match rules::geofence(
        &scheduled.loc_lat,
        &scheduled.loc_long,
        scheduled.loc_radius,
        (body.latitude, body.longitude),
    ) {
        Geofence::Outside {
            distance_m,
            radius_m,
        } => return Err(check::out_of_radius(distance_m, radius_m)),
        Geofence::Inside {
            distance_m,
            radius_m,
        } => (Some(distance_m.round() as i32), Some(radius_m)),
        Geofence::Unrestricted => (None, None),
    };

    let note = clean_string(body.note.as_deref());
    let pending_key = format!("{}|{}", user.email.to_lowercase(), now_local.date());
    let window = check::time_window(scheduled);
    let new = NewRequest {
        email: &user.email,
        name: user.name.as_deref().unwrap_or("N/A"),
        group_uid: &group_uid,
        reason: &body.reason,
        note: note.as_deref(),
        absen_type: kind.absen_type(),
        latitude: body.latitude,
        longitude: body.longitude,
        distance_m,
        radius_m,
        schedule_id: scheduled.schedule_id,
        shift_uid: &scheduled.shift_uid,
        shift_name: &scheduled.shift_name,
        shift_window: &window,
        loc_name: &scheduled.loc_name,
        failed_attempts: i32::try_from(failed).unwrap_or(i32::MAX),
        pending_key: &pending_key,
        at: now.with_timezone(&Utc),
    };
    let id = match repository::insert(&state.db, &new).await {
        Ok(id) => id,
        Err(sqlx::Error::Database(e)) if e.is_unique_violation() => {
            return Err(conflict(
                "Anda sudah punya permintaan absen manual yang menunggu keputusan admin hari ini.",
            ));
        }
        Err(err) => return Err(err.into()),
    };
    let row = fetch(&state, &group_uid, i32::try_from(id).unwrap_or(i32::MAX)).await?;
    created(to_item(row), "Permintaan absen manual berhasil dikirim")
}

async fn fetch(state: &AppState, group_uid: &str, id: i32) -> Result<Row, ApiError> {
    repository::find(&state.db, group_uid, id)
        .await?
        .ok_or_else(not_found)
}

fn admin_group(user: &CurrentUser) -> Result<String, ApiError> {
    user.require_group_uid().map(str::to_owned)
}

pub async fn list(
    ValidatedQuery(query): ValidatedQuery<ListQuery>,
    user: CurrentUser,
    State(state): State<AppState>,
) -> ApiResult<Vec<ManualItem>> {
    user.require_role(&ADMIN_ROLES)?;
    let group_uid = admin_group(&user)?;
    let page = resolve_pagination(PaginationParams {
        limit: query.limit.and_then(Numeric::value),
        offset: query.offset.and_then(Numeric::value),
    });
    let rows = repository::list(&state.db, &group_uid, query.status.as_deref(), page).await?;
    ok(
        rows.into_iter().map(to_item).collect(),
        "Daftar permintaan absen manual berhasil diambil",
    )
}

pub async fn decide(
    AbsenId(id): AbsenId,
    State(state): State<AppState>,
    AuthedJson(auth, body): AuthedJson<DecisionBody>,
) -> ApiResult<ManualItem> {
    let user = auth.require_role(&ADMIN_ROLES)?;
    let group_uid = admin_group(&user)?;
    let row = fetch(&state, &group_uid, id).await?;
    if row.status != STATUS_PENDING {
        return Err(conflict("Permintaan ini sudah diputuskan."));
    }
    let approve = body.action == "approve";
    let note = clean_string(body.note.as_deref());
    let decision = Decision {
        id,
        group_uid: &group_uid,
        status: if approve {
            STATUS_APPROVED
        } else {
            STATUS_REJECTED
        },
        by: &user.email,
        note: note.as_deref(),
        at: Utc::now().trunc_subsecs(0),
    };

    let mut tx = state.db.begin().await?;
    if !repository::decide_in(&mut tx, &decision).await? {
        return Err(conflict("Permintaan ini sudah diputuskan."));
    }
    if approve {
        let absen_id = approve_in(&state, &mut tx, &group_uid, &row).await?;
        repository::link_absen(&mut tx, id, absen_id).await?;
    }
    tx.commit().await?;
    if approve {
        let day = row.requested_at.with_timezone(&Local).date_naive();
        clear_failures(&state, &row.user_email, day).await;
    }

    let item = to_item(fetch(&state, &group_uid, id).await?);
    ok(
        item,
        if approve {
            "Permintaan absen manual disetujui"
        } else {
            "Permintaan absen manual ditolak"
        },
    )
}

/// The attendance row for an approved request, decided as of the request time against the
/// schedule as it is now. The request is stale (409) if that no longer gives the same shift and
/// the same IN/OUT as when it was filed, e.g. the guard managed to check in normally meanwhile.
async fn approve_in(
    state: &AppState,
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
    group_uid: &str,
    row: &Row,
) -> Result<u64, ApiError> {
    let stale = || {
        conflict(
            "Jadwal atau status absen petugas sudah berubah sejak permintaan dibuat. Tolak permintaan ini.",
        )
    };
    let at = row.requested_at.with_timezone(&Local);
    let at_local = at.naive_local();
    let found = schedules::around(&state.db, group_uid, &row.user_email, at_local.date()).await?;
    let slots: Vec<_> = found.iter().map(|s| s.slot.clone()).collect();
    let (index, kind) = rules::decide(&slots, at_local).map_err(|_| stale())?;
    let scheduled = &found[index];
    if scheduled.schedule_id != row.schedule_id || kind.absen_type() != row.absen_type {
        return Err(stale());
    }

    let tolerance = &state.config.attendance;
    let (late, early) = rules::lateness(
        kind,
        &scheduled.slot,
        at_local,
        tolerance.late_tolerance_minutes,
        tolerance.early_leave_tolerance_minutes,
    );
    let absen = NewAbsen {
        email: &row.user_email,
        name: &row.user_name,
        at: row.requested_at,
        kind,
        lat: check::format_coordinate(row.latitude),
        long: check::format_coordinate(row.longitude),
        late_mins: late,
        early_mins: early,
        location: &check::location_text(None, &scheduled.loc_name),
        shift_uid: &scheduled.shift_uid,
        group_uid,
        shift_start: scheduled.slot.start.format("%H:%M:%S").to_string(),
        face_verified: false,
        face_score: None,
        photo_path: None,
        clock: at_local.format("%H:%M:%S").to_string(),
    };
    schedules::record_in(tx, scheduled.schedule_id, &absen)
        .await?
        .ok_or_else(stale)
}
