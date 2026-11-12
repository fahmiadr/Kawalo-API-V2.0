//! Reads and writes of the schedule forecast. Shifts, members and rows are read with whatever
//! connection the caller passes, so the commit reads inside its own transaction. The group lock is
//! `manage_repository::lock_group`, shared with the manual write endpoints.

use chrono::{Datelike, NaiveDate};
use sqlx::{MySql, MySqlConnection, QueryBuilder};

use super::generator::{ExistingRow, ShiftSpec};
use super::repository::date_key;
use crate::core::clean::clean_string;

/// Live (not soft-deleted) shifts of the group among `uids`, in no particular order.
pub async fn shifts(
    conn: &mut MySqlConnection,
    group_uid: &str,
    uids: &[&str],
) -> sqlx::Result<Vec<ShiftSpec>> {
    let mut q = QueryBuilder::<MySql>::new(
        "SELECT ShiftUID, ShiftName, ShiftTimeStart, ShiftTimeStart_mm FROM tkshift \
         WHERE ShiftDeletedAt IS NULL AND ShiftGroupUID = ",
    );
    q.push_bind(group_uid).push(" AND ShiftUID IN (");
    let mut list = q.separated(", ");
    for uid in uids {
        list.push_bind(*uid);
    }
    q.push(")");
    let rows: Vec<(String, String, i32, i32)> = q.build_query_as().fetch_all(conn).await?;
    Ok(rows
        .into_iter()
        .map(|(uid, name, start_hour, start_min)| ShiftSpec {
            uid,
            name,
            start_hour,
            start_min,
            quota: 0,
        })
        .collect())
}

#[derive(Debug, Clone)]
pub struct Member {
    pub email: String,
    pub name: String,
    pub role: String,
}

/// JOINED members of the group, sorted by email; the name falls back to the email.
pub async fn members(conn: &mut MySqlConnection, group_uid: &str) -> sqlx::Result<Vec<Member>> {
    let rows: Vec<(Option<String>, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT ug.Email, u.UserName, ug.Category FROM tkusergroup ug \
         LEFT JOIN tkuser u ON u.UserEmail = ug.Email \
         WHERE ug.GroupUID = ? AND ug.Status = 'JOINED' ORDER BY ug.Email",
    )
    .bind(group_uid)
    .fetch_all(conn)
    .await?;
    Ok(rows
        .into_iter()
        .filter_map(|(email, name, role)| {
            let email = clean_string(email.as_deref())?;
            Some(Member {
                name: clean_string(name.as_deref()).unwrap_or_else(|| email.clone()),
                role: role.unwrap_or_default().trim().to_uppercase(),
                email,
            })
        })
        .collect())
}

fn filled(raw: Option<&str>) -> bool {
    clean_string(raw).is_some()
}

/// Group rows dated `from..=to`, with the start of their own shift (also a deleted one, so its
/// code stays right). Rows that are not calendar dates are dropped. Locked = before `today`, or
/// already checked in or out.
pub async fn existing(
    conn: &mut MySqlConnection,
    group_uid: &str,
    from: NaiveDate,
    to: NaiveDate,
    today: NaiveDate,
) -> sqlx::Result<Vec<ExistingRow>> {
    type Row = (
        i32,
        i32,
        i32,
        i32,
        String,
        String,
        Option<i32>,
        Option<i32>,
        Option<String>,
        Option<String>,
    );
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT s.SchedId, s.SYear, s.SMonth, s.SDate, s.SchedShiftUID, s.SFriendEmail, \
         h.ShiftTimeStart, h.ShiftTimeStart_mm, s.ACheckin, s.ACheckout \
         FROM tkschedule s \
         LEFT JOIN tkshift h ON h.ShiftUID = s.SchedShiftUID AND h.ShiftGroupUID = s.SGroupUID \
         WHERE s.SGroupUID = ? AND s.SYear BETWEEN ? AND ? \
         AND s.SYear * 10000 + s.SMonth * 100 + s.SDate BETWEEN ? AND ? \
         ORDER BY s.SchedId",
    )
    .bind(group_uid)
    .bind(from.year())
    .bind(to.year())
    .bind(date_key(from))
    .bind(date_key(to))
    .fetch_all(conn)
    .await?;
    Ok(rows
        .into_iter()
        .filter_map(|(id, y, m, d, shift_uid, email, h, mm, cin, cout)| {
            let date = NaiveDate::from_ymd_opt(y, u32::try_from(m).ok()?, u32::try_from(d).ok()?)?;
            Some(ExistingRow {
                id: i64::from(id),
                date,
                shift_uid,
                email,
                shift_start: h.map(|h| (h, mm.unwrap_or(0))),
                locked: date < today || filled(cin.as_deref()) || filled(cout.as_deref()),
            })
        })
        .collect())
}

/// Hard delete like the admin panel; ids come from [`existing`] read in the same transaction.
pub async fn delete(conn: &mut MySqlConnection, group_uid: &str, ids: &[i64]) -> sqlx::Result<u64> {
    let mut deleted = 0;
    for chunk in ids.chunks(500) {
        let mut q = QueryBuilder::<MySql>::new("DELETE FROM tkschedule WHERE SGroupUID = ");
        q.push_bind(group_uid).push(" AND SchedId IN (");
        let mut list = q.separated(", ");
        for id in chunk {
            list.push_bind(*id);
        }
        q.push(")");
        deleted += q.build().execute(&mut *conn).await?.rows_affected();
    }
    Ok(deleted)
}

pub struct NewRow<'a> {
    pub date: NaiveDate,
    pub shift_uid: &'a str,
    pub email: &'a str,
    pub name: &'a str,
}

/// Multi-row insert in chunks of 500. `SchedDT` is the literal `YYYY-MM-DD 01:00:00` the legacy
/// Android app searches with; the other defaults match the admin panel.
pub async fn insert(
    conn: &mut MySqlConnection,
    group_uid: &str,
    rows: &[NewRow<'_>],
) -> sqlx::Result<u64> {
    let mut created = 0;
    for chunk in rows.chunks(500) {
        let mut q = QueryBuilder::<MySql>::new(
            "INSERT INTO tkschedule (SchedShiftUID, SDate, SMonth, SYear, SFriendEmail, \
             SFriendName, SGroupUID, SchedDT, LateMins, ACheckin, ACheckout) ",
        );
        q.push_values(chunk, |mut b, r| {
            b.push_bind(r.shift_uid)
                .push_bind(r.date.day())
                .push_bind(r.date.month())
                .push_bind(r.date.year())
                .push_bind(r.email)
                .push_bind(r.name)
                .push_bind(group_uid)
                .push_bind(format!("{} 01:00:00", r.date.format("%Y-%m-%d")))
                .push_bind(0)
                .push_bind("N/A")
                .push_bind("N/A");
        });
        created += q.build().execute(&mut *conn).await?.rows_affected();
    }
    Ok(created)
}
