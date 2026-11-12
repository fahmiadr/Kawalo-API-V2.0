use chrono::NaiveDate;
use sqlx::{FromRow, MySqlPool};

use super::entity::ScheduleItem;
use crate::core::clean::clean_string;

#[derive(Debug, FromRow)]
struct Row {
    id: i32,
    day: i32,
    month: i32,
    year: i32,
    shift_uid: String,
    shift_name: String,
    shift_color: String,
    start_hour: i32,
    start_min: i32,
    stop_hour: i32,
    stop_min: i32,
    location: String,
    checkin: Option<String>,
    checkout: Option<String>,
    late_mins: i32,
}

pub(super) fn date_key(date: NaiveDate) -> i32 {
    use chrono::Datelike;
    date.year() * 10_000 + i32::try_from(date.month() * 100 + date.day()).unwrap_or(0)
}

fn into_item(row: Row) -> Option<ScheduleItem> {
    let date = NaiveDate::from_ymd_opt(
        row.year,
        u32::try_from(row.month).ok()?,
        u32::try_from(row.day).ok()?,
    )?;
    Some(ScheduleItem {
        id: row.id.to_string(),
        date: date.format("%Y-%m-%d").to_string(),
        shift_uid: row.shift_uid,
        shift_name: row.shift_name,
        shift_color: row.shift_color,
        start_time: format!("{:02}:{:02}", row.start_hour, row.start_min),
        end_time: format!("{:02}:{:02}", row.stop_hour, row.stop_min),
        location_name: clean_string(Some(&row.location)),
        checkin: clean_string(row.checkin.as_deref()),
        checkout: clean_string(row.checkout.as_deref()),
        late_mins: row.late_mins,
    })
}

/// The year/month/day integers are compared as `YYYYMMDD`, which is monotonic for valid dates;
/// rows that are not real calendar dates (`20260231`) can fall inside the range and are dropped
/// afterwards. The shift must belong to the same group as the schedule row.
pub async fn list(
    pool: &MySqlPool,
    group_uid: &str,
    email: &str,
    from: NaiveDate,
    to: NaiveDate,
) -> Result<Vec<ScheduleItem>, sqlx::Error> {
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT s.SchedId AS id, s.SDate AS day, s.SMonth AS month, s.SYear AS year, \
         h.ShiftUID AS shift_uid, h.ShiftName AS shift_name, h.ShiftColor AS shift_color, \
         h.ShiftTimeStart AS start_hour, h.ShiftTimeStart_mm AS start_min, \
         h.ShiftTimeStop AS stop_hour, h.ShiftTimeStop_mm AS stop_min, h.LocName AS location, \
         s.ACheckin AS checkin, s.ACheckout AS checkout, s.LateMins AS late_mins \
         FROM tkschedule s \
         JOIN tkshift h ON h.ShiftUID = s.SchedShiftUID AND h.ShiftGroupUID = s.SGroupUID \
         WHERE s.SGroupUID = ? AND s.SFriendEmail = ? \
         AND s.SYear * 10000 + s.SMonth * 100 + s.SDate BETWEEN ? AND ? \
         ORDER BY s.SYear, s.SMonth, s.SDate, h.ShiftTimeStart, h.ShiftTimeStart_mm, s.SchedId",
    )
    .bind(group_uid)
    .bind(email)
    .bind(date_key(from))
    .bind(date_key(to))
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().filter_map(into_item).collect())
}
