//! `GET /schedules/me` against the scratch services. Each test uses its own group and emails.

mod common;

use axum::http::StatusCode;
use chrono::{Datelike, Days, Local};
use common::TestApp;
use serde_json::Value;
use sqlx::MySqlPool;

fn unique() -> String {
    uuid::Uuid::new_v4().simple().to_string()[..12].to_owned()
}

async fn insert_shift(
    db: &MySqlPool,
    group: &str,
    uid: &str,
    name: &str,
    start: (i32, i32),
    loc: &str,
) {
    sqlx::query(
        "INSERT INTO tkshift (ShiftName, ShiftTimeStart, ShiftTimeStart_mm, ShiftTimeStop, \
         ShiftTimeStop_mm, ShiftGroupUID, ShiftColor, ShiftUID, LocName) \
         VALUES (?, ?, ?, 16, 30, ?, '#112233', ?, ?)",
    )
    .bind(name)
    .bind(start.0)
    .bind(start.1)
    .bind(group)
    .bind(uid)
    .bind(loc)
    .execute(db)
    .await
    .expect("insert shift");
}

/// `(year, month, day)` as stored by the legacy app.
type Ymd = (i32, i32, i32);

async fn insert_schedule(
    db: &MySqlPool,
    group: &str,
    email: &str,
    shift: &str,
    ymd: Ymd,
    checkin: &str,
    late: i32,
) -> u64 {
    sqlx::query(
        "INSERT INTO tkschedule (SchedShiftUID, SDate, SMonth, SYear, SFriendEmail, SFriendName, \
         SGroupUID, LateMins, ACheckin) VALUES (?, ?, ?, ?, ?, 'Nama', ?, ?, ?)",
    )
    .bind(shift)
    .bind(ymd.2)
    .bind(ymd.1)
    .bind(ymd.0)
    .bind(email)
    .bind(group)
    .bind(late)
    .bind(checkin)
    .execute(db)
    .await
    .expect("insert schedule")
    .last_insert_id()
}

struct Fixture {
    app: TestApp,
    group: String,
    email: String,
    token: String,
}

async fn fixture() -> Fixture {
    let app = TestApp::start().await;
    let tag = unique();
    let group = format!("g{tag}");
    let email = format!("budi.{tag}@test.id");
    let token = app.token_for(&email, &group, false).await;
    Fixture {
        app,
        group,
        email,
        token,
    }
}

async fn get(f: &Fixture, query: &str) -> Vec<Value> {
    let (status, body) = f
        .app
        .call(
            "GET",
            &format!("/api/v1/schedules/me{query}"),
            Some(&f.token),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["success"], true);
    assert_eq!(body["message"], "Jadwal berhasil diambil");
    body["data"].as_array().expect("data array").clone()
}

#[tokio::test]
async fn requires_a_token() {
    let f = fixture().await;
    let (status, body) = f.app.call("GET", "/api/v1/schedules/me", None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["error_code"], "AUTH_UNAUTHORIZED");
}

#[tokio::test]
async fn empty_range_is_an_empty_list() {
    let f = fixture().await;
    assert!(get(&f, "").await.is_empty());
    assert!(get(&f, "?from=2020-01-01&to=2020-01-31").await.is_empty());
}

#[tokio::test]
async fn bad_or_oversized_ranges_are_422() {
    let f = fixture().await;
    for q in [
        "from=2026-06-01&to=2026-05-31",
        "from=2026-01-01&to=2026-03-05",
        "from=2026-6-3",
        "from=2026-02-30",
        "to=garbage",
        "from=",
    ] {
        let (status, body) = f
            .app
            .call(
                "GET",
                &format!("/api/v1/schedules/me?{q}"),
                Some(&f.token),
                None,
            )
            .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{q}: {body}");
        assert_eq!(body["error_code"], "VALIDATION_ERROR", "{q}");
        assert_eq!(body["message"], "Format data masukan tidak valid", "{q}");
    }
    // Validation runs before authentication, like the other query routes.
    let (status, _) = f
        .app
        .call("GET", "/api/v1/schedules/me?from=bad", None, None)
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn lists_own_rows_in_range_sorted_with_cleaned_fields() {
    let f = fixture().await;
    let db = &f.app.state.db;
    let (g, tag) = (&f.group, unique());
    let (pagi, siang, pos) = (format!("pg{tag}"), format!("sg{tag}"), format!("ps{tag}"));
    insert_shift(db, g, &pagi, "Shift Pagi", (8, 0), "N/A").await;
    insert_shift(db, g, &siang, "Shift Siang", (14, 5), "Pos 1").await;
    insert_shift(db, g, &pos, "Shift Dini", (6, 0), "N/A").await;

    let late_id = insert_schedule(
        db,
        g,
        &f.email,
        &siang,
        (2026, 6, 30),
        "2026-06-30 14:10",
        5,
    )
    .await;
    let first_id = insert_schedule(db, g, &f.email, &pagi, (2026, 6, 30), "N/A", 0).await;
    let dini_id = insert_schedule(db, g, &f.email, &pos, (2026, 7, 1), "", 0).await;
    insert_schedule(db, g, &f.email, &pagi, (2026, 7, 2), "N/A", 0).await; // outside the range
    insert_schedule(db, g, &f.email, &pagi, (2026, 2, 31), "N/A", 0).await; // not a calendar date
    insert_schedule(db, g, &f.email, &pos, (2026, 13, 1), "N/A", 0).await;

    let rows = get(&f, "?from=2026-06-30&to=2026-07-01").await;
    let ids: Vec<_> = rows.iter().map(|r| r["id"].as_str().expect("id")).collect();
    assert_eq!(
        ids,
        [
            first_id.to_string(),
            late_id.to_string(),
            dini_id.to_string()
        ]
    );

    let r = &rows[0];
    assert_eq!(r["date"], "2026-06-30");
    assert_eq!(r["shift_uid"], pagi);
    assert_eq!(r["shift_name"], "Shift Pagi");
    assert_eq!(r["shift_color"], "#112233");
    assert_eq!(r["start_time"], "08:00");
    assert_eq!(r["end_time"], "16:30");
    assert!(r["location_name"].is_null());
    assert!(r["checkin"].is_null());
    assert!(r["checkout"].is_null());
    assert_eq!(r["late_mins"], 0);

    assert_eq!(rows[1]["start_time"], "14:05");
    assert_eq!(rows[1]["location_name"], "Pos 1");
    assert_eq!(rows[1]["checkin"], "2026-06-30 14:10");
    assert_eq!(rows[1]["late_mins"], 5);
    assert!(rows[2]["checkin"].is_null(), "empty string becomes null");
    assert_eq!(rows[2]["start_time"], "06:00");
}

#[tokio::test]
async fn excludes_other_users_and_other_groups() {
    let f = fixture().await;
    let db = &f.app.state.db;
    let (g, tag) = (&f.group, unique());
    let (mine, other_group) = (format!("m{tag}"), format!("og{tag}"));
    insert_shift(db, g, &mine, "Shift", (8, 0), "N/A").await;
    let foreign_shift = format!("fs{tag}");
    insert_shift(db, &other_group, &foreign_shift, "Asing", (8, 0), "N/A").await;

    let own = insert_schedule(db, g, &f.email, &mine, (2026, 6, 30), "N/A", 0).await;
    insert_schedule(
        db,
        g,
        &format!("siti.{tag}@test.id"),
        &mine,
        (2026, 6, 30),
        "N/A",
        0,
    )
    .await;
    insert_schedule(
        db,
        &other_group,
        &f.email,
        &foreign_shift,
        (2026, 6, 30),
        "N/A",
        0,
    )
    .await;
    // Same group, but the shift lives in another group: not a valid join.
    insert_schedule(db, g, &f.email, &foreign_shift, (2026, 7, 1), "N/A", 0).await;

    let rows = get(&f, "?from=2026-06-30&to=2026-07-01").await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["id"], own.to_string());
}

#[tokio::test]
async fn default_range_starts_today_and_spans_30_days() {
    let f = fixture().await;
    let db = &f.app.state.db;
    let tag = unique();
    let shift = format!("d{tag}");
    insert_shift(db, &f.group, &shift, "Shift", (8, 0), "N/A").await;
    let today = Local::now().date_naive();
    let ymd = |days_from_today: i64| {
        let d = if days_from_today >= 0 {
            today.checked_add_days(Days::new(days_from_today.unsigned_abs()))
        } else {
            today.checked_sub_days(Days::new(days_from_today.unsigned_abs()))
        }
        .expect("date");
        (
            d.year(),
            i32::try_from(d.month()).expect("month"),
            i32::try_from(d.day()).expect("day"),
        )
    };
    insert_schedule(db, &f.group, &f.email, &shift, ymd(-1), "N/A", 0).await;
    let first = insert_schedule(db, &f.group, &f.email, &shift, ymd(0), "N/A", 0).await;
    let last = insert_schedule(db, &f.group, &f.email, &shift, ymd(30), "N/A", 0).await;
    insert_schedule(db, &f.group, &f.email, &shift, ymd(31), "N/A", 0).await;

    let rows = get(&f, "").await;
    let ids: Vec<_> = rows.iter().map(|r| r["id"].as_str().expect("id")).collect();
    assert_eq!(ids, [first.to_string(), last.to_string()]);
}

#[tokio::test]
async fn the_group_attendance_point_replaces_the_shift_location() {
    let f = fixture().await;
    let db = &f.app.state.db;
    let (g, shift) = (&f.group, format!("sh{}", unique()));
    insert_shift(db, g, &shift, "Shift Pagi", (8, 0), "Pos 1").await;
    insert_schedule(db, g, &f.email, &shift, (2026, 6, 30), "N/A", 0).await;
    let range = "?from=2026-06-30&to=2026-06-30";
    assert_eq!(get(&f, range).await[0]["location_name"], "Pos 1");

    let insert = |name: &'static str| {
        sqlx::query(
            "INSERT INTO tkattendancelocation (GroupUID, LocName, LocLat, LocLong, LocRadius, \
             UpdatedBy) VALUES (?, ?, -6.2, 106.8, 50, 'admin@test.id') \
             ON DUPLICATE KEY UPDATE LocName = VALUES(LocName)",
        )
        .bind(g)
        .bind(name)
        .execute(db)
    };
    insert("Gerbang Barat").await.expect("group point");
    assert_eq!(get(&f, range).await[0]["location_name"], "Gerbang Barat");
    // A group point without a name hides the shift's name too: the point wins as a whole.
    insert("N/A").await.expect("unnamed group point");
    assert!(get(&f, range).await[0]["location_name"].is_null());
}
