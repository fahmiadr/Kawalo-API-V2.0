use chrono::NaiveDateTime;
use sqlx::{FromRow, MySqlPool};

use super::dto::{NA, ShiftInput, ShiftItem, parse_coordinate, span};
use crate::core::clean::clean_string;

#[derive(Debug, FromRow)]
struct Row {
    uid: String,
    name: String,
    start_hour: i32,
    start_min: i32,
    stop_hour: i32,
    stop_min: i32,
    color: String,
    description: Option<String>,
    location_name: String,
    lat: String,
    lng: String,
    radius: i32,
}

macro_rules! select {
    ($tail:literal) => {
        concat!(
            "SELECT ShiftUID AS uid, ShiftName AS name, ShiftTimeStart AS start_hour, \
     ShiftTimeStart_mm AS start_min, ShiftTimeStop AS stop_hour, ShiftTimeStop_mm AS stop_min, \
     ShiftColor AS color, ShiftDesc AS description, LocName AS location_name, \
     LocLat AS lat, LocLong AS lng, LocRadius AS radius FROM tkshift \
     WHERE ShiftGroupUID = ? AND ShiftDeletedAt IS NULL",
            $tail
        )
    };
}

impl From<Row> for ShiftItem {
    fn from(row: Row) -> Self {
        let start = row.start_hour * 60 + row.start_min;
        let end = row.stop_hour * 60 + row.stop_min;
        let (crosses_midnight, duration_mins) = span(start, end);
        let coordinates = parse_coordinate(&row.lat, 90.0)
            .zip(parse_coordinate(&row.lng, 180.0))
            .filter(|&(lat, long)| lat != 0.0 || long != 0.0);
        ShiftItem {
            uid: row.uid,
            name: row.name.trim().to_owned(),
            start_time: format!("{:02}:{:02}", row.start_hour, row.start_min),
            end_time: format!("{:02}:{:02}", row.stop_hour, row.stop_min),
            crosses_midnight,
            duration_mins,
            color: row.color,
            description: clean_string(row.description.as_deref()),
            location_name: clean_string(Some(&row.location_name)),
            glat: coordinates.map(|c| c.0),
            glong: coordinates.map(|c| c.1),
            radius: coordinates.map(|_| row.radius),
        }
    }
}

pub async fn list(
    pool: &MySqlPool,
    group_uid: &str,
    search: Option<&str>,
) -> Result<Vec<ShiftItem>, sqlx::Error> {
    let rows: Vec<Row> = sqlx::query_as(select!(
        " AND (? IS NULL OR ShiftName LIKE ?) \
         ORDER BY ShiftTimeStart, ShiftTimeStart_mm, ShiftName, ShiftUID"
    ))
    .bind(group_uid)
    .bind(search)
    .bind(search.map(|s| format!("%{s}%")))
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(Into::into).collect())
}

pub async fn find(
    pool: &MySqlPool,
    group_uid: &str,
    uid: &str,
) -> Result<Option<ShiftItem>, sqlx::Error> {
    let row: Option<Row> = sqlx::query_as(select!(" AND ShiftUID = ?"))
        .bind(group_uid)
        .bind(uid)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(Into::into))
}

/// Case-insensitive name clash among the group's active shifts, ignoring `except_uid`.
// ponytail: check-then-write without a unique index (legacy duplicates exist); two admins saving
// the same name in the same instant can both pass. Add a locking read if that ever bites.
pub async fn name_taken(
    pool: &MySqlPool,
    group_uid: &str,
    name: &str,
    except_uid: Option<&str>,
) -> Result<bool, sqlx::Error> {
    let found: Option<i32> = sqlx::query_scalar(
        "SELECT 1 FROM tkshift WHERE ShiftGroupUID = ? AND ShiftDeletedAt IS NULL \
         AND LOWER(TRIM(ShiftName)) = LOWER(?) AND ShiftUID <> ? LIMIT 1",
    )
    .bind(group_uid)
    .bind(name)
    .bind(except_uid.unwrap_or_default())
    .fetch_optional(pool)
    .await?;
    Ok(found.is_some())
}

pub async fn insert(
    pool: &MySqlPool,
    group_uid: &str,
    uid: &str,
    shift: &ShiftInput,
) -> Result<(), sqlx::Error> {
    let (lat, long) = shift.location.as_ref().map_or((NA, NA), |(a, b)| (a, b));
    sqlx::query(
        "INSERT INTO tkshift (ShiftUID, ShiftGroupUID, ShiftName, ShiftTimeStart, ShiftTimeStart_mm, \
         ShiftTimeStop, ShiftTimeStop_mm, ShiftColor, ShiftDesc, LocName, LocLat, LocLong, LocRadius) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(uid)
    .bind(group_uid)
    .bind(&shift.name)
    .bind(shift.start.0)
    .bind(shift.start.1)
    .bind(shift.end.0)
    .bind(shift.end.1)
    .bind(&shift.color)
    .bind(&shift.description)
    .bind(&shift.location_name)
    .bind(lat)
    .bind(long)
    .bind(shift.radius)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn replace(
    pool: &MySqlPool,
    group_uid: &str,
    uid: &str,
    shift: &ShiftInput,
) -> Result<(), sqlx::Error> {
    let (lat, long) = shift.location.as_ref().map_or((NA, NA), |(a, b)| (a, b));
    sqlx::query(
        "UPDATE tkshift SET ShiftName = ?, ShiftTimeStart = ?, ShiftTimeStart_mm = ?, \
         ShiftTimeStop = ?, ShiftTimeStop_mm = ?, ShiftColor = ?, ShiftDesc = ?, LocName = ?, \
         LocLat = ?, LocLong = ?, LocRadius = ? \
         WHERE ShiftUID = ? AND ShiftGroupUID = ? AND ShiftDeletedAt IS NULL",
    )
    .bind(&shift.name)
    .bind(shift.start.0)
    .bind(shift.start.1)
    .bind(shift.end.0)
    .bind(shift.end.1)
    .bind(&shift.color)
    .bind(&shift.description)
    .bind(&shift.location_name)
    .bind(lat)
    .bind(long)
    .bind(shift.radius)
    .bind(uid)
    .bind(group_uid)
    .execute(pool)
    .await?;
    Ok(())
}

/// Schedules of the shift on `from_key` (`YYYYMMDD`) or later.
pub async fn count_schedules_from(
    pool: &MySqlPool,
    group_uid: &str,
    uid: &str,
    from_key: i32,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM tkschedule WHERE SGroupUID = ? AND SchedShiftUID = ? \
         AND SYear * 10000 + SMonth * 100 + SDate >= ?",
    )
    .bind(group_uid)
    .bind(uid)
    .bind(from_key)
    .fetch_one(pool)
    .await
}

pub async fn soft_delete(
    pool: &MySqlPool,
    group_uid: &str,
    uid: &str,
    now: NaiveDateTime,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE tkshift SET ShiftDeletedAt = ? \
         WHERE ShiftUID = ? AND ShiftGroupUID = ? AND ShiftDeletedAt IS NULL",
    )
    .bind(now)
    .bind(uid)
    .bind(group_uid)
    .execute(pool)
    .await?;
    Ok(())
}
