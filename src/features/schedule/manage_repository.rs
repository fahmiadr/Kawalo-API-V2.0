use std::collections::{HashMap, HashSet};

use axum::http::StatusCode;
use chrono::{Datelike, NaiveDate};
use sha2::{Digest, Sha256};
use sqlx::pool::PoolConnection;
use sqlx::{FromRow, MySql, MySqlConnection, MySqlPool, QueryBuilder};

use super::entity::ManagedItem;
use super::manage_plan::{Existing, Occupancy};
use super::repository::date_key;
use crate::core::clean::clean_string;
use crate::core::error::{ApiError, ErrorCode};

const CHUNK: usize = 500;

const MSG_BUSY: &str = "Jadwal grup ini sedang diubah. Coba lagi sebentar lagi.";
pub const MSG_EXISTS: &str = "Jadwal personil ini pada tanggal dan shift tersebut sudah ada.";

pub fn conflict(message: &str) -> ApiError {
    ApiError::new(StatusCode::CONFLICT, ErrorCode::ConflictError, message)
}

/// The unique key is the last line of defence behind the application-level checks.
pub fn map_write(error: sqlx::Error) -> ApiError {
    if error
        .as_database_error()
        .is_some_and(|e| e.is_unique_violation())
    {
        conflict(MSG_EXISTS)
    } else {
        error.into()
    }
}

/// Hashed in Rust, not with SQL `MD5()`: the production MySQL 9 build has no such function.
/// 16 + 32 characters stay within MySQL's 64 character lock-name limit for any group uid.
fn lock_name(group: &str) -> String {
    let digest = hex::encode(Sha256::digest(group.as_bytes()));
    format!("kawalo:schedule:{}", &digest[..32])
}

/// Serialises writers of one group. Named locks belong to the connection, so the caller must hand
/// the connection back through [`release_group`] on every path.
pub async fn lock_group(pool: &MySqlPool, group: &str) -> Result<PoolConnection<MySql>, ApiError> {
    let mut conn = pool.acquire().await?;
    let got: Option<i64> = sqlx::query_scalar("SELECT GET_LOCK(?, 10)")
        .bind(lock_name(group))
        .fetch_one(&mut *conn)
        .await?;
    if got != Some(1) {
        return Err(conflict(MSG_BUSY));
    }
    Ok(conn)
}

pub async fn release_group(mut conn: PoolConnection<MySql>, group: &str) {
    let released = sqlx::query("SELECT RELEASE_LOCK(?)")
        .bind(lock_name(group))
        .execute(&mut *conn)
        .await;
    if released.is_err() {
        // A connection that may still hold the lock must not go back into the pool.
        conn.close_on_drop();
    }
}

fn push_in(query: &mut QueryBuilder<MySql>, values: &[String]) {
    query.push("(");
    let mut list = query.separated(", ");
    for value in values {
        list.push_bind(value.as_str());
    }
    query.push(")");
}

/// Active (`JOINED`) members among `emails`, keyed by lower-cased email, as `(email, name)`.
pub async fn members(
    conn: &mut MySqlConnection,
    group: &str,
    emails: &[String],
) -> Result<HashMap<String, (String, String)>, sqlx::Error> {
    let mut found = HashMap::new();
    for chunk in emails.chunks(CHUNK) {
        let mut query = QueryBuilder::<MySql>::new(
            "SELECT g.Email, COALESCE(NULLIF(TRIM(u.UserName), ''), g.Email) \
             FROM tkusergroup g LEFT JOIN tkuser u ON u.UserEmail = g.Email \
             WHERE g.Status = 'JOINED' AND g.GroupUID = ",
        );
        query.push_bind(group).push(" AND g.Email IN ");
        push_in(&mut query, chunk);
        let rows: Vec<(String, String)> = query.build_query_as().fetch_all(&mut *conn).await?;
        for (email, name) in rows {
            found.insert(email.to_lowercase(), (email, name));
        }
    }
    Ok(found)
}

/// Shifts of this group that are not soft-deleted.
pub async fn valid_shifts(
    conn: &mut MySqlConnection,
    group: &str,
    uids: &[String],
) -> Result<HashSet<String>, sqlx::Error> {
    let mut found = HashSet::new();
    for chunk in uids.chunks(CHUNK) {
        let mut query = QueryBuilder::<MySql>::new(
            "SELECT ShiftUID FROM tkshift WHERE ShiftDeletedAt IS NULL AND ShiftGroupUID = ",
        );
        query.push_bind(group).push(" AND ShiftUID IN ");
        push_in(&mut query, chunk);
        let rows: Vec<String> = query.build_query_scalar().fetch_all(&mut *conn).await?;
        found.extend(rows);
    }
    Ok(found)
}

pub fn filled(value: Option<&str>) -> bool {
    clean_string(value).is_some()
}

fn to_date(year: i32, month: i32, day: i32) -> Option<NaiveDate> {
    NaiveDate::from_ymd_opt(year, u32::try_from(month).ok()?, u32::try_from(day).ok()?)
}

/// Rows of `emails` in `from..=to`, grouped per person and day.
pub async fn occupancy(
    conn: &mut MySqlConnection,
    group: &str,
    emails: &[String],
    from: NaiveDate,
    to: NaiveDate,
) -> Result<Occupancy, sqlx::Error> {
    type Row = (
        i32,
        String,
        String,
        i32,
        i32,
        i32,
        Option<String>,
        Option<String>,
    );
    let mut found = Occupancy::new();
    for chunk in emails.chunks(CHUNK) {
        let mut query = QueryBuilder::<MySql>::new(
            "SELECT SchedId, SFriendEmail, SchedShiftUID, SYear, SMonth, SDate, ACheckin, ACheckout \
             FROM tkschedule WHERE SGroupUID = ",
        );
        query
            .push_bind(group)
            .push(" AND SYear BETWEEN ")
            .push_bind(from.year())
            .push(" AND ")
            .push_bind(to.year())
            .push(" AND SYear * 10000 + SMonth * 100 + SDate BETWEEN ")
            .push_bind(date_key(from))
            .push(" AND ")
            .push_bind(date_key(to))
            .push(" AND SFriendEmail IN ");
        push_in(&mut query, chunk);
        let rows: Vec<Row> = query.build_query_as().fetch_all(&mut *conn).await?;
        for (id, email, shift_uid, y, m, d, checkin, checkout) in rows {
            let Some(date) = to_date(y, m, d) else {
                continue;
            };
            found
                .entry((email.to_lowercase(), date))
                .or_default()
                .push(Existing {
                    id: Some(id),
                    shift_uid,
                    locked: filled(checkin.as_deref()) || filled(checkout.as_deref()),
                });
        }
    }
    Ok(found)
}

pub struct NewRow<'a> {
    pub shift_uid: &'a str,
    pub email: &'a str,
    pub name: &'a str,
    pub date: NaiveDate,
}

/// Returns the generated id of the last row inserted, which is the row itself for a single insert.
pub async fn insert_many(
    conn: &mut MySqlConnection,
    group: &str,
    rows: &[NewRow<'_>],
) -> Result<u64, ApiError> {
    let mut last_id = 0;
    for chunk in rows.chunks(CHUNK) {
        let mut query = QueryBuilder::<MySql>::new(
            "INSERT INTO tkschedule (SchedShiftUID, SDate, SMonth, SYear, SFriendEmail, \
             SFriendName, SGroupUID, SchedDT, LateMins, ACheckin, ACheckout) ",
        );
        query.push_values(chunk, |mut values, row| {
            // The legacy Android app looks schedules up by this literal timestamp.
            let stamp = format!("{} 01:00:00", row.date.format("%Y-%m-%d"));
            values
                .push_bind(row.shift_uid)
                .push_bind(i32::try_from(row.date.day()).unwrap_or(0))
                .push_bind(i32::try_from(row.date.month()).unwrap_or(0))
                .push_bind(row.date.year())
                .push_bind(row.email)
                .push_bind(row.name)
                .push_bind(group)
                .push_bind(stamp)
                .push_bind(0_i32)
                .push_bind("N/A")
                .push_bind("N/A");
        });
        last_id = query
            .build()
            .execute(&mut *conn)
            .await
            .map_err(map_write)?
            .last_insert_id();
    }
    Ok(last_id)
}

pub async fn update_row(
    conn: &mut MySqlConnection,
    group: &str,
    id: i32,
    row: &NewRow<'_>,
) -> Result<(), ApiError> {
    let stamp = format!("{} 01:00:00", row.date.format("%Y-%m-%d"));
    sqlx::query(
        "UPDATE tkschedule SET SchedShiftUID = ?, SDate = ?, SMonth = ?, SYear = ?, \
         SFriendEmail = ?, SFriendName = ?, SchedDT = ? WHERE SchedId = ? AND SGroupUID = ?",
    )
    .bind(row.shift_uid)
    .bind(i32::try_from(row.date.day()).unwrap_or(0))
    .bind(i32::try_from(row.date.month()).unwrap_or(0))
    .bind(row.date.year())
    .bind(row.email)
    .bind(row.name)
    .bind(stamp)
    .bind(id)
    .bind(group)
    .execute(conn)
    .await
    .map_err(map_write)?;
    Ok(())
}

pub async fn delete_ids(
    conn: &mut MySqlConnection,
    group: &str,
    ids: &[i32],
) -> Result<u64, sqlx::Error> {
    let mut deleted = 0;
    for chunk in ids.chunks(CHUNK) {
        let mut query = QueryBuilder::<MySql>::new("DELETE FROM tkschedule WHERE SGroupUID = ");
        query.push_bind(group).push(" AND SchedId IN (");
        let mut list = query.separated(", ");
        for id in chunk {
            list.push_bind(*id);
        }
        query.push(")");
        deleted += query.build().execute(&mut *conn).await?.rows_affected();
    }
    Ok(deleted)
}

/// Deletes only rows that are still unlocked at execution time, so a check-in that lands between
/// the caller's read and this statement is never destroyed.
pub async fn delete_unlocked(
    conn: &mut MySqlConnection,
    group: &str,
    ids: &[i32],
    today: NaiveDate,
) -> Result<u64, sqlx::Error> {
    let mut deleted = 0;
    for chunk in ids.chunks(CHUNK) {
        let mut query = QueryBuilder::<MySql>::new(
            "DELETE FROM tkschedule WHERE COALESCE(NULLIF(TRIM(ACheckin), ''), 'N/A') IN ('N/A', 'null') \
             AND COALESCE(NULLIF(TRIM(ACheckout), ''), 'N/A') IN ('N/A', 'null') \
             AND SYear * 10000 + SMonth * 100 + SDate >= ",
        );
        query
            .push_bind(date_key(today))
            .push(" AND SGroupUID = ")
            .push_bind(group)
            .push(" AND SchedId IN (");
        let mut list = query.separated(", ");
        for id in chunk {
            list.push_bind(*id);
        }
        query.push(")");
        deleted += query.build().execute(&mut *conn).await?.rows_affected();
    }
    Ok(deleted)
}

pub struct Stored {
    pub id: i32,
    pub email: String,
    pub shift_uid: String,
    pub date: NaiveDate,
    pub locked: bool,
}

/// Rows of the group among `ids`; rows whose date is not a real calendar date are left out.
pub async fn stored(
    conn: &mut MySqlConnection,
    group: &str,
    ids: &[i32],
    today: NaiveDate,
) -> Result<Vec<Stored>, sqlx::Error> {
    type Row = (
        i32,
        String,
        String,
        i32,
        i32,
        i32,
        Option<String>,
        Option<String>,
    );
    let mut found = Vec::new();
    for chunk in ids.chunks(CHUNK) {
        let mut query = QueryBuilder::<MySql>::new(
            "SELECT SchedId, SFriendEmail, SchedShiftUID, SYear, SMonth, SDate, ACheckin, ACheckout \
             FROM tkschedule WHERE SGroupUID = ",
        );
        query.push_bind(group).push(" AND SchedId IN (");
        let mut list = query.separated(", ");
        for id in chunk {
            list.push_bind(*id);
        }
        query.push(")");
        let rows: Vec<Row> = query.build_query_as().fetch_all(&mut *conn).await?;
        for (id, email, shift_uid, y, m, d, checkin, checkout) in rows {
            let Some(date) = to_date(y, m, d) else {
                continue;
            };
            found.push(Stored {
                id,
                email,
                shift_uid,
                date,
                locked: date < today || filled(checkin.as_deref()) || filled(checkout.as_deref()),
            });
        }
    }
    Ok(found)
}

#[derive(Debug, FromRow)]
struct ItemRow {
    id: i32,
    day: i32,
    month: i32,
    year: i32,
    email: String,
    name: String,
    avatar: Option<String>,
    is_member: i64,
    shift_uid: Option<String>,
    shift_name: Option<String>,
    shift_color: Option<String>,
    start_hour: Option<i32>,
    start_min: Option<i32>,
    stop_hour: Option<i32>,
    stop_min: Option<i32>,
    location: Option<String>,
    checkin: Option<String>,
    checkout: Option<String>,
    late_mins: i32,
}

fn clock(hour: Option<i32>, minute: Option<i32>) -> Option<String> {
    Some(format!("{:02}:{:02}", hour?, minute?))
}

fn into_item(row: ItemRow, today: NaiveDate) -> Option<ManagedItem> {
    let date = to_date(row.year, row.month, row.day)?;
    let checkin = clean_string(row.checkin.as_deref());
    let checkout = clean_string(row.checkout.as_deref());
    Some(ManagedItem {
        id: row.id.to_string(),
        date: date.format("%Y-%m-%d").to_string(),
        name: clean_string(Some(&row.name)).unwrap_or_else(|| row.email.clone()),
        email: row.email,
        avatar_url: clean_string(row.avatar.as_deref()),
        is_member: row.is_member != 0,
        shift_uid: row.shift_uid,
        shift_name: row.shift_name,
        shift_color: row.shift_color,
        start_time: clock(row.start_hour, row.start_min),
        end_time: clock(row.stop_hour, row.stop_min),
        location_name: clean_string(row.location.as_deref()),
        locked: date < today || checkin.is_some() || checkout.is_some(),
        checkin,
        checkout,
        late_mins: row.late_mins,
    })
}

#[derive(Default)]
pub struct Filter<'a> {
    pub id: Option<i32>,
    pub range: Option<(NaiveDate, NaiveDate)>,
    pub email: Option<&'a str>,
    pub shift_uid: Option<&'a str>,
}

/// The shift is a LEFT JOIN: rows whose shift was hard-deleted by the legacy admin stay visible
/// so they can be removed.
pub async fn items(
    pool: &MySqlPool,
    group: &str,
    today: NaiveDate,
    filter: &Filter<'_>,
) -> Result<Vec<ManagedItem>, sqlx::Error> {
    let mut query = QueryBuilder::<MySql>::new(
        "SELECT s.SchedId AS id, s.SDate AS day, s.SMonth AS month, s.SYear AS year, \
         s.SFriendEmail AS email, s.SFriendName AS name, u.UserProfilePic AS avatar, \
         CAST(EXISTS(SELECT 1 FROM tkusergroup g WHERE g.GroupUID = s.SGroupUID \
         AND g.Email = s.SFriendEmail AND g.Status = 'JOINED') AS SIGNED) AS is_member, \
         h.ShiftUID AS shift_uid, h.ShiftName AS shift_name, h.ShiftColor AS shift_color, \
         h.ShiftTimeStart AS start_hour, h.ShiftTimeStart_mm AS start_min, \
         h.ShiftTimeStop AS stop_hour, h.ShiftTimeStop_mm AS stop_min, h.LocName AS location, \
         s.ACheckin AS checkin, s.ACheckout AS checkout, s.LateMins AS late_mins \
         FROM tkschedule s \
         LEFT JOIN tkshift h ON h.ShiftUID = s.SchedShiftUID AND h.ShiftGroupUID = s.SGroupUID \
         LEFT JOIN tkuser u ON u.UserEmail = s.SFriendEmail \
         WHERE s.SGroupUID = ",
    );
    query.push_bind(group);
    if let Some(id) = filter.id {
        query.push(" AND s.SchedId = ").push_bind(id);
    }
    if let Some((from, to)) = filter.range {
        query
            .push(" AND s.SYear BETWEEN ")
            .push_bind(from.year())
            .push(" AND ")
            .push_bind(to.year())
            .push(" AND s.SYear * 10000 + s.SMonth * 100 + s.SDate BETWEEN ")
            .push_bind(date_key(from))
            .push(" AND ")
            .push_bind(date_key(to));
    }
    if let Some(email) = filter.email {
        query.push(" AND s.SFriendEmail = ").push_bind(email);
    }
    if let Some(uid) = filter.shift_uid {
        query.push(" AND s.SchedShiftUID = ").push_bind(uid);
    }
    query.push(
        " ORDER BY s.SYear, s.SMonth, s.SDate, h.ShiftTimeStart, h.ShiftTimeStart_mm, \
         s.SFriendName, s.SchedId",
    );
    let rows: Vec<ItemRow> = query.build_query_as().fetch_all(pool).await?;
    Ok(rows
        .into_iter()
        .filter_map(|row| into_item(row, today))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::lock_name;

    #[test]
    fn lock_name_fits_mysql_limit_for_any_group_uid() {
        let long = "g".repeat(500);
        for group in ["", "grpuid20250610_142621_veranda", long.as_str()] {
            let name = lock_name(group);
            assert!(name.len() <= 64, "{name}");
            assert!(name.starts_with("kawalo:schedule:"));
        }
        assert_ne!(lock_name("a"), lock_name("b"));
        assert_eq!(lock_name("a"), lock_name("a"));
    }
}
