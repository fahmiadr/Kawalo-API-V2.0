use chrono::{DateTime, Utc};
use sqlx::{FromRow, MySql, MySqlPool, QueryBuilder, Transaction};

use crate::core::pagination::Pagination;

pub struct NewRequest<'a> {
    pub email: &'a str,
    pub name: &'a str,
    pub group_uid: &'a str,
    pub reason: &'a str,
    pub note: Option<&'a str>,
    pub absen_type: &'a str,
    pub latitude: f64,
    pub longitude: f64,
    pub distance_m: Option<i32>,
    pub radius_m: Option<i32>,
    pub schedule_id: i32,
    pub shift_uid: &'a str,
    pub shift_name: &'a str,
    pub shift_window: &'a str,
    pub loc_name: &'a str,
    pub failed_attempts: i32,
    /// Unique while the request is open; see the migration.
    pub pending_key: &'a str,
    pub at: DateTime<Utc>,
}

#[derive(Debug, FromRow)]
pub struct Row {
    pub id: i32,
    pub status: String,
    pub reason: String,
    pub note: Option<String>,
    pub user_email: String,
    pub user_name: String,
    pub absen_type: String,
    pub latitude: f64,
    pub longitude: f64,
    pub distance_m: Option<i32>,
    pub radius_m: Option<i32>,
    pub schedule_id: i32,
    pub shift_uid: String,
    pub shift_name: String,
    pub shift_window: String,
    pub loc_name: String,
    pub failed_attempts: i32,
    pub requested_at: DateTime<Utc>,
    pub decided_by: Option<String>,
    pub decided_at: Option<DateTime<Utc>>,
    pub decision_note: Option<String>,
    pub absen_id: Option<i32>,
}

const SELECT: &str = "SELECT ManualId AS id, Status AS status, Reason AS reason, Note AS note, \
    UserEmail AS user_email, UserName AS user_name, AbsenType AS absen_type, \
    Latitude AS latitude, Longitude AS longitude, DistanceM AS distance_m, RadiusM AS radius_m, \
    ScheduleId AS schedule_id, ShiftUID AS shift_uid, ShiftName AS shift_name, \
    ShiftWindow AS shift_window, LocName AS loc_name, FailedAttempts AS failed_attempts, \
    RequestedAt AS requested_at, DecidedBy AS decided_by, DecidedAt AS decided_at, \
    DecisionNote AS decision_note, AbsenID AS absen_id FROM tkattendancemanual WHERE GroupUID = ";

/// A unique violation means the user already has an open request today.
pub async fn insert(db: &MySqlPool, new: &NewRequest<'_>) -> sqlx::Result<u64> {
    sqlx::query(
        "INSERT INTO tkattendancemanual (UserEmail, UserName, GroupUID, Reason, Note, AbsenType, \
         Latitude, Longitude, DistanceM, RadiusM, ScheduleId, ShiftUID, ShiftName, ShiftWindow, \
         LocName, FailedAttempts, Status, PendingKey, RequestedAt) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'PENDING', ?, ?)",
    )
    .bind(new.email)
    .bind(new.name)
    .bind(new.group_uid)
    .bind(new.reason)
    .bind(new.note)
    .bind(new.absen_type)
    .bind(new.latitude)
    .bind(new.longitude)
    .bind(new.distance_m)
    .bind(new.radius_m)
    .bind(new.schedule_id)
    .bind(new.shift_uid)
    .bind(new.shift_name)
    .bind(new.shift_window)
    .bind(new.loc_name)
    .bind(new.failed_attempts)
    .bind(new.pending_key)
    .bind(new.at)
    .execute(db)
    .await
    .map(|done| done.last_insert_id())
}

pub async fn find(db: &MySqlPool, group_uid: &str, id: i32) -> sqlx::Result<Option<Row>> {
    let mut query = QueryBuilder::<MySql>::new(SELECT);
    query
        .push_bind(group_uid)
        .push(" AND ManualId = ")
        .push_bind(id);
    query.build_query_as().fetch_optional(db).await
}

pub async fn list(
    db: &MySqlPool,
    group_uid: &str,
    status: Option<&str>,
    page: Pagination,
) -> sqlx::Result<Vec<Row>> {
    let mut query = QueryBuilder::<MySql>::new(SELECT);
    query.push_bind(group_uid);
    if let Some(status) = status {
        query.push(" AND Status = ").push_bind(status);
    }
    query.push(" ORDER BY RequestedAt DESC, ManualId DESC");
    if let (Some(take), Some(skip)) = (page.take(), page.skip()) {
        query.push(" LIMIT ").push_bind(take);
        query.push(" OFFSET ").push_bind(skip);
    }
    query.build_query_as().fetch_all(db).await
}

pub struct Decision<'a> {
    pub id: i32,
    pub group_uid: &'a str,
    pub status: &'a str,
    pub by: &'a str,
    pub note: Option<&'a str>,
    pub at: DateTime<Utc>,
}

/// Closes the request; `false` when it was not pending (decided concurrently) or not in the group.
pub async fn decide_in(
    tx: &mut Transaction<'_, MySql>,
    decision: &Decision<'_>,
) -> sqlx::Result<bool> {
    let done = sqlx::query(
        "UPDATE tkattendancemanual SET Status = ?, PendingKey = NULL, DecidedBy = ?, \
         DecidedAt = ?, DecisionNote = ? WHERE ManualId = ? AND GroupUID = ? AND Status = 'PENDING'",
    )
    .bind(decision.status)
    .bind(decision.by)
    .bind(decision.at)
    .bind(decision.note)
    .bind(decision.id)
    .bind(decision.group_uid)
    .execute(&mut **tx)
    .await?;
    Ok(done.rows_affected() == 1)
}

pub async fn link_absen(
    tx: &mut Transaction<'_, MySql>,
    id: i32,
    absen_id: u64,
) -> sqlx::Result<()> {
    sqlx::query("UPDATE tkattendancemanual SET AbsenID = ? WHERE ManualId = ?")
        .bind(absen_id)
        .bind(id)
        .execute(&mut **tx)
        .await
        .map(drop)
}
