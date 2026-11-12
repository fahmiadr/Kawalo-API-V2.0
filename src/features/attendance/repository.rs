use std::collections::{HashMap, HashSet};

use sqlx::{FromRow, MySql, MySqlPool, QueryBuilder};

use super::entity::{AttendanceItem, Category, shift_time_window};
use crate::core::clean::{clean_coordinate, clean_string};
use crate::features::period_query::{PeriodFilter, clean_timestamp_text};

#[derive(Debug, FromRow)]
struct AbsenRow {
    id: i32,
    email: Option<String>,
    name: Option<String>,
    taken_at: Option<String>,
    kind: Option<String>,
    lat: Option<String>,
    long: Option<String>,
    diff_mins: Option<i32>,
    early_mins: i32,
    face_verified: i64,
    has_photo: i64,
    face_score: Option<f32>,
    manual: i64,
    location: Option<String>,
    shift_uid: Option<String>,
    group_uid: Option<String>,
    sh_start: Option<String>,
}

#[derive(Debug, FromRow)]
struct UserRow {
    email: Option<String>,
    name: Option<String>,
    pic: Option<String>,
    duty: Option<String>,
}

#[derive(Debug, FromRow)]
struct ShiftRow {
    uid: Option<String>,
    name: Option<String>,
    color: Option<String>,
    start_hour: Option<i32>,
    start_minute: Option<i32>,
    stop_hour: Option<i32>,
    stop_minute: Option<i32>,
}

/// Distinct cleaned values of one column, in first-seen order.
fn distinct_clean<'a>(values: impl Iterator<Item = &'a Option<String>>) -> Vec<String> {
    let mut seen = HashSet::new();
    values
        .filter_map(|v| clean_string(v.as_deref()))
        .filter(|v| seen.insert(v.clone()))
        .collect()
}

fn push_in_list(query: &mut QueryBuilder<MySql>, values: &[String]) {
    query.push(" IN (");
    let mut list = query.separated(", ");
    for value in values {
        list.push_bind(value.clone());
    }
    query.push(")");
}

/// Attendance rows of the group, newest first, enriched with group, user and shift data.
pub async fn list(
    pool: &MySqlPool,
    filter: &PeriodFilter<'_>,
    with_scores: bool,
) -> Result<Vec<AttendanceItem>, sqlx::Error> {
    let mut matching_shifts = Vec::new();
    let mut pattern = None;
    if let Some(search) = &filter.search {
        let like = format!("%{search}%");
        let uids: Vec<Option<String>> = sqlx::query_scalar(
            "SELECT ShiftUID FROM tkshift WHERE ShiftGroupUID = ? AND ShiftName LIKE ?",
        )
        .bind(filter.group_uid)
        .bind(&like)
        .fetch_all(pool)
        .await?;
        matching_shifts = distinct_clean(uids.iter());
        pattern = Some(like);
    }

    let mut query = QueryBuilder::<MySql>::new(
        "SELECT AbsenID AS id, AbsenEmail AS email, AbsenName AS name, \
         CAST(AbsenDT AS CHAR) AS taken_at, AbsenType AS kind, ALat AS lat, ALong AS `long`, \
         ADiffMins AS diff_mins, AEarlyMins AS early_mins, \
         CAST(AFaceVerified AS SIGNED) AS face_verified, \
         AFaceScore AS face_score, \
         CAST(APhotoPath IS NOT NULL AND TRIM(APhotoPath) <> '' AS SIGNED) AS has_photo, \
         CAST(EXISTS(SELECT 1 FROM tkattendancemanual m WHERE m.AbsenID = tkabsen.AbsenID) AS SIGNED) AS manual, \
         ALocation AS location, AShiftUID AS shift_uid, \
         AbsenGroupUID AS group_uid, AShStart AS sh_start \
         FROM tkabsen WHERE AbsenGroupUID = ",
    );
    query.push_bind(filter.group_uid);
    if let Some(from) = filter.from {
        query.push(" AND AbsenDT >= ").push_bind(from);
    }
    if let Some(to) = filter.to {
        query.push(" AND AbsenDT <= ").push_bind(to);
    }
    if let Some(like) = &pattern {
        query.push(" AND (");
        for (i, column) in ["AbsenName", "AbsenEmail", "AbsenType", "ALocation"]
            .into_iter()
            .enumerate()
        {
            if i > 0 {
                query.push(" OR ");
            }
            query.push(column).push(" LIKE ").push_bind(like.clone());
        }
        if !matching_shifts.is_empty() {
            query.push(" OR AShiftUID");
            push_in_list(&mut query, &matching_shifts);
        }
        query.push(")");
    }
    query.push(" ORDER BY AbsenDT DESC");
    if let (Some(take), Some(skip)) = (filter.page.take(), filter.page.skip()) {
        query.push(" LIMIT ").push_bind(take);
        query.push(" OFFSET ").push_bind(skip);
    }
    let records: Vec<AbsenRow> = query.build_query_as().fetch_all(pool).await?;
    if records.is_empty() {
        return Ok(Vec::new());
    }

    let group_uids = distinct_clean(records.iter().map(|r| &r.group_uid));
    let emails = distinct_clean(records.iter().map(|r| &r.email));
    let shift_uids = distinct_clean(records.iter().map(|r| &r.shift_uid));
    let (groups, users, shifts) = tokio::try_join!(
        fetch_group_names(pool, &group_uids),
        fetch_users(pool, &emails),
        fetch_shifts(pool, filter.group_uid, &shift_uids),
    )?;

    Ok(records
        .into_iter()
        .filter_map(|r| to_item(r, &groups, &users, &shifts, with_scores))
        .collect())
}

async fn fetch_group_names(
    pool: &MySqlPool,
    uids: &[String],
) -> Result<HashMap<String, Option<String>>, sqlx::Error> {
    if uids.is_empty() {
        return Ok(HashMap::new());
    }
    let mut query =
        QueryBuilder::<MySql>::new("SELECT GroupUID, GroupName FROM tkgroup WHERE GroupUID");
    push_in_list(&mut query, uids);
    let rows: Vec<(Option<String>, Option<String>)> =
        query.build_query_as().fetch_all(pool).await?;
    Ok(rows
        .into_iter()
        .filter_map(|(uid, name)| {
            Some((clean_string(uid.as_deref())?, clean_string(name.as_deref())))
        })
        .collect())
}

async fn fetch_users(
    pool: &MySqlPool,
    emails: &[String],
) -> Result<HashMap<String, UserRow>, sqlx::Error> {
    if emails.is_empty() {
        return Ok(HashMap::new());
    }
    let mut query = QueryBuilder::<MySql>::new(
        "SELECT UserEmail AS email, UserName AS name, UserProfilePic AS pic, UserDuty AS duty \
         FROM tkuser WHERE UserEmail",
    );
    push_in_list(&mut query, emails);
    let rows: Vec<UserRow> = query.build_query_as().fetch_all(pool).await?;
    Ok(rows
        .into_iter()
        .filter_map(|u| Some((clean_string(u.email.as_deref())?, u)))
        .collect())
}

async fn fetch_shifts(
    pool: &MySqlPool,
    group_uid: &str,
    uids: &[String],
) -> Result<HashMap<String, ShiftRow>, sqlx::Error> {
    if uids.is_empty() {
        return Ok(HashMap::new());
    }
    let mut query = QueryBuilder::<MySql>::new(
        "SELECT ShiftUID AS uid, ShiftName AS name, ShiftColor AS color, \
         ShiftTimeStart AS start_hour, ShiftTimeStart_mm AS start_minute, \
         ShiftTimeStop AS stop_hour, ShiftTimeStop_mm AS stop_minute \
         FROM tkshift WHERE ShiftGroupUID = ",
    );
    query.push_bind(group_uid);
    query.push(" AND ShiftUID");
    push_in_list(&mut query, uids);
    let rows: Vec<ShiftRow> = query.build_query_as().fetch_all(pool).await?;
    Ok(rows
        .into_iter()
        .filter_map(|s| Some((clean_string(s.uid.as_deref())?, s)))
        .collect())
}

/// `None` for rows without a usable id, which the API drops instead of inventing one.
fn to_item(
    record: AbsenRow,
    groups: &HashMap<String, Option<String>>,
    users: &HashMap<String, UserRow>,
    shifts: &HashMap<String, ShiftRow>,
    with_scores: bool,
) -> Option<AttendanceItem> {
    if record.id <= 0 {
        return None;
    }
    let group_uid = clean_string(record.group_uid.as_deref());
    let user_email = clean_string(record.email.as_deref());
    let shift_uid = clean_string(record.shift_uid.as_deref());

    let group_name = group_uid.and_then(|g| groups.get(&g)?.clone());
    let user = user_email.as_ref().and_then(|e| users.get(e));
    let shift = shift_uid.and_then(|s| shifts.get(&s));

    let hours = shift.and_then(|s| Some((i64::from(s.start_hour?), i64::from(s.stop_hour?))));
    let minute = |value: Option<i32>| value.map_or(0, i64::from);
    let time_window = shift_time_window(
        hours,
        minute(shift.and_then(|s| s.start_minute)),
        minute(shift.and_then(|s| s.stop_minute)),
        record.sh_start.as_deref(),
    );

    let attendance_type = clean_string(record.kind.as_deref());
    Some(AttendanceItem {
        id: record.id.to_string(),
        user_name: user
            .and_then(|u| clean_string(u.name.as_deref()))
            .or_else(|| clean_string(record.name.as_deref())),
        user_profile_pic: user.and_then(|u| clean_string(u.pic.as_deref())),
        avatar_url: user.and_then(|u| clean_string(u.pic.as_deref())),
        user_duty: user.and_then(|u| clean_string(u.duty.as_deref())),
        user_email,
        attendance_dt: clean_timestamp_text(record.taken_at.as_deref()),
        category: Category::of(attendance_type.as_deref()),
        attendance_type,
        latitude: clean_coordinate(record.lat.as_deref()),
        longitude: clean_coordinate(record.long.as_deref()),
        diff_minutes: record.diff_mins.map_or(0, i64::from),
        early_leave_minutes: i64::from(record.early_mins),
        face_verified: if record.face_verified != 0 {
            Some(true)
        } else if record.manual != 0 {
            Some(false)
        } else {
            None
        },
        has_photo: record.has_photo != 0,
        face_similarity_percent: record
            .face_score
            .filter(|_| with_scores)
            .map(super::check::percent),
        location_name: clean_string(record.location.as_deref()),
        shift_name: shift.and_then(|s| clean_string(s.name.as_deref())),
        shift_color: shift.and_then(|s| clean_string(s.color.as_deref())),
        shift_time_window: time_window,
        group_name,
    })
}
