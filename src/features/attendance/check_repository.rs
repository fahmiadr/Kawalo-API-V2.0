use chrono::{DateTime, Datelike, NaiveDate, NaiveTime, Utc};
use sqlx::{FromRow, MySql, MySqlPool, Transaction};

use super::rules::{Kind, Slot};
use crate::core::clean::clean_string;

#[derive(Debug, FromRow)]
struct Row {
    id: i32,
    day: i32,
    month: i32,
    year: i32,
    checkin: Option<String>,
    checkout: Option<String>,
    shift_uid: String,
    shift_name: String,
    start_hour: i32,
    start_min: i32,
    stop_hour: i32,
    stop_min: i32,
    loc_name: String,
    loc_lat: String,
    loc_long: String,
    loc_radius: i32,
}

/// A schedule row of the caller with its (non-deleted) shift.
#[derive(Debug, Clone)]
pub struct Scheduled {
    pub schedule_id: i32,
    pub slot: Slot,
    pub shift_uid: String,
    pub shift_name: String,
    pub loc_name: String,
    pub loc_lat: String,
    pub loc_long: String,
    pub loc_radius: i32,
}

fn date_key(date: NaiveDate) -> i32 {
    date.year() * 10_000 + i32::try_from(date.month() * 100 + date.day()).unwrap_or(0)
}

fn time(hour: i32, minute: i32) -> Option<NaiveTime> {
    NaiveTime::from_hms_opt(u32::try_from(hour).ok()?, u32::try_from(minute).ok()?, 0)
}

fn into_scheduled(row: Row) -> Option<Scheduled> {
    let date = NaiveDate::from_ymd_opt(
        row.year,
        u32::try_from(row.month).ok()?,
        u32::try_from(row.day).ok()?,
    )?;
    Some(Scheduled {
        schedule_id: row.id,
        slot: Slot {
            date,
            start: time(row.start_hour, row.start_min)?,
            stop: time(row.stop_hour, row.stop_min)?,
            checked_in: clean_string(row.checkin.as_deref()).is_some(),
            checked_out: clean_string(row.checkout.as_deref()).is_some(),
        },
        shift_uid: row.shift_uid,
        shift_name: row.shift_name,
        loc_name: row.loc_name,
        loc_lat: row.loc_lat,
        loc_long: row.loc_long,
        loc_radius: row.loc_radius,
    })
}

/// The caller's shifts on `today` and the day before (for shifts that end after midnight).
/// Rows with impossible dates or times are dropped.
pub async fn around(
    db: &MySqlPool,
    group_uid: &str,
    email: &str,
    today: NaiveDate,
) -> sqlx::Result<Vec<Scheduled>> {
    let yesterday = today.pred_opt().unwrap_or(today);
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT s.SchedId AS id, s.SDate AS day, s.SMonth AS month, s.SYear AS year, \
         s.ACheckin AS checkin, s.ACheckout AS checkout, \
         h.ShiftUID AS shift_uid, h.ShiftName AS shift_name, \
         h.ShiftTimeStart AS start_hour, h.ShiftTimeStart_mm AS start_min, \
         h.ShiftTimeStop AS stop_hour, h.ShiftTimeStop_mm AS stop_min, \
         h.LocName AS loc_name, h.LocLat AS loc_lat, h.LocLong AS loc_long, \
         h.LocRadius AS loc_radius \
         FROM tkschedule s \
         JOIN tkshift h ON h.ShiftUID = s.SchedShiftUID AND h.ShiftGroupUID = s.SGroupUID \
         WHERE s.SGroupUID = ? AND s.SFriendEmail = ? AND h.ShiftDeletedAt IS NULL \
         AND s.SYear * 10000 + s.SMonth * 100 + s.SDate IN (?, ?) \
         ORDER BY s.SYear, s.SMonth, s.SDate, h.ShiftTimeStart, h.ShiftTimeStart_mm, s.SchedId",
    )
    .bind(group_uid)
    .bind(email)
    .bind(date_key(today))
    .bind(date_key(yesterday))
    .fetch_all(db)
    .await?;
    Ok(rows.into_iter().filter_map(into_scheduled).collect())
}

pub struct NewAbsen<'a> {
    pub email: &'a str,
    pub name: &'a str,
    pub at: DateTime<Utc>,
    pub kind: Kind,
    pub lat: String,
    pub long: String,
    pub late_mins: i32,
    pub early_mins: i32,
    pub location: &'a str,
    pub shift_uid: &'a str,
    pub group_uid: &'a str,
    pub shift_start: String,
    /// `false` for a manual row approved by an admin: no photo, no score.
    pub face_verified: bool,
    pub face_score: Option<f32>,
    pub photo_path: Option<&'a str>,
    /// Local wall-clock `HH:MM:SS` written to the schedule, as the legacy admin did.
    pub clock: String,
}

/// [`record_in`] in a transaction of its own.
pub async fn record(
    db: &MySqlPool,
    schedule_id: i32,
    absen: &NewAbsen<'_>,
) -> sqlx::Result<Option<u64>> {
    let mut tx = db.begin().await?;
    let id = record_in(&mut tx, schedule_id, absen).await?;
    if id.is_some() {
        tx.commit().await?;
    } else {
        tx.rollback().await?;
    }
    Ok(id)
}

/// Marks the schedule and inserts the attendance row. The schedule update is conditional on the
/// state [`decide`](super::rules::decide) saw, so of two concurrent requests only one wins;
/// `None` means this one lost and the caller must roll back.
pub async fn record_in(
    tx: &mut Transaction<'_, MySql>,
    schedule_id: i32,
    absen: &NewAbsen<'_>,
) -> sqlx::Result<Option<u64>> {
    let marked = match absen.kind {
        Kind::In => {
            sqlx::query(
                "UPDATE tkschedule SET ACheckin = ?, LateMins = ? WHERE SchedId = ? \
                 AND (ACheckin IS NULL OR TRIM(ACheckin) IN ('', 'N/A', 'null'))",
            )
            .bind(&absen.clock)
            .bind(absen.late_mins)
            .bind(schedule_id)
            .execute(&mut **tx)
            .await?
        }
        Kind::Out => {
            sqlx::query(
                "UPDATE tkschedule SET ACheckout = ? WHERE SchedId = ? \
                 AND (ACheckout IS NULL OR TRIM(ACheckout) IN ('', 'N/A', 'null')) \
                 AND NOT (ACheckin IS NULL OR TRIM(ACheckin) IN ('', 'N/A', 'null'))",
            )
            .bind(&absen.clock)
            .bind(schedule_id)
            .execute(&mut **tx)
            .await?
        }
    };
    if marked.rows_affected() != 1 {
        return Ok(None);
    }
    let id = sqlx::query(
        "INSERT INTO tkabsen (AbsenEmail, AbsenName, AbsenDT, AbsenType, ALat, ALong, ADiffMins, \
         AEarlyMins, ALocation, AShiftUID, AbsenGroupUID, AShStart, AFaceVerified, AFaceScore, \
         APhotoPath) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(absen.email)
    .bind(absen.name)
    .bind(absen.at)
    .bind(absen.kind.absen_type())
    .bind(&absen.lat)
    .bind(&absen.long)
    .bind(absen.late_mins)
    .bind(absen.early_mins)
    .bind(absen.location)
    .bind(absen.shift_uid)
    .bind(absen.group_uid)
    .bind(&absen.shift_start)
    .bind(absen.face_verified)
    .bind(absen.face_score)
    .bind(absen.photo_path)
    .execute(&mut **tx)
    .await?
    .last_insert_id();
    Ok(Some(id))
}

#[derive(Debug, FromRow)]
pub struct PhotoRef {
    pub email: Option<String>,
    pub group_uid: Option<String>,
    pub photo_path: Option<String>,
}

pub async fn photo_ref(db: &MySqlPool, id: i32) -> sqlx::Result<Option<PhotoRef>> {
    sqlx::query_as(
        "SELECT AbsenEmail AS email, AbsenGroupUID AS group_uid, APhotoPath AS photo_path \
         FROM tkabsen WHERE AbsenID = ?",
    )
    .bind(id)
    .fetch_optional(db)
    .await
}
