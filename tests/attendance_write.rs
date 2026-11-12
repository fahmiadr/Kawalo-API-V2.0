//! `POST /attendance` (check-in/out), its nonce, the admin photo download and the stats, against
//! the scratch services with the fake face engine. Each test uses its own group and emails.
//!
//! Shifts run 00:00-23:59 today, so a check-in is late and a check-out early at any time of day
//! (unless the tolerance is raised). A run across local midnight can fail spuriously.

mod common;

use axum::http::{StatusCode, header};
use chrono::{Datelike, Local};
use common::faces::{self, BLACK, GREEN, RED, face_jpeg, join, register, unique};
use common::{Part, TestApp};
use serde_json::Value;
use tempfile::TempDir;

const LAT: &str = "-6.2000000";
const LONG: &str = "106.8000000";

struct Fixture {
    app: TestApp,
    _photos: TempDir,
    group: String,
    email: String,
    token: String,
    shift: String,
}

async fn insert_shift(app: &TestApp, group: &str, uid: &str, radius: i32) {
    sqlx::query(
        "INSERT INTO tkshift (ShiftName, ShiftTimeStart, ShiftTimeStart_mm, ShiftTimeStop, \
         ShiftTimeStop_mm, ShiftGroupUID, ShiftColor, ShiftUID, LocName, LocLat, LocLong, LocRadius) \
         VALUES ('Shift Uji', 0, 0, 23, 59, ?, '#112233', ?, 'Pos Utama', ?, ?, ?)",
    )
    .bind(group)
    .bind(uid)
    .bind(LAT)
    .bind(LONG)
    .bind(radius)
    .execute(&app.state.db)
    .await
    .expect("insert shift");
}

async fn insert_today(app: &TestApp, group: &str, email: &str, shift: &str) -> u64 {
    let today = Local::now().date_naive();
    sqlx::query(
        "INSERT INTO tkschedule (SchedShiftUID, SDate, SMonth, SYear, SFriendEmail, SFriendName, \
         SGroupUID) VALUES (?, ?, ?, ?, ?, 'Nama', ?)",
    )
    .bind(shift)
    .bind(i32::try_from(today.day()).expect("day"))
    .bind(i32::try_from(today.month()).expect("month"))
    .bind(today.year())
    .bind(email)
    .bind(group)
    .execute(&app.state.db)
    .await
    .expect("insert schedule")
    .last_insert_id()
}

/// A guard with a registered face (red), a 100 m geofenced shift and a schedule today.
async fn fixture_with(tolerance: Option<i64>) -> Fixture {
    let photos = tempfile::tempdir().expect("photo dir");
    let dir = photos.path().to_owned();
    let app = TestApp::start_with_state(
        |config| {
            faces::configure(config, &dir);
            if let Some(minutes) = tolerance {
                config.attendance.late_tolerance_minutes = minutes;
                config.attendance.early_leave_tolerance_minutes = minutes;
            }
        },
        |state| {
            state.faces = kawalo_api::core::face::Faces::with_engine(
                std::sync::Arc::new(faces::FakeFaceEngine),
                &state.config.faces,
            );
        },
    )
    .await;
    let tag = unique();
    let group = format!("g{tag}");
    let email = format!("guard.{tag}@test.id");
    let shift = format!("s{tag}");
    join(&app, &group, &email, "GUARD").await;
    insert_shift(&app, &group, &shift, 100).await;
    insert_today(&app, &group, &email, &shift).await;
    let token = app.token_for(&email, &group, false).await;
    assert_eq!(register(&app, &token, RED).await.0, StatusCode::CREATED);
    Fixture {
        app,
        _photos: photos,
        group,
        email,
        token,
        shift,
    }
}

async fn fixture() -> Fixture {
    fixture_with(None).await
}

async fn nonce(app: &TestApp, token: &str) -> String {
    let (code, body) = app
        .call("POST", "/api/v1/attendance/nonce", Some(token), None)
        .await;
    assert_eq!(code, StatusCode::CREATED, "{body}");
    assert_eq!(body["data"]["expires_in"], 120);
    body["data"]["nonce"].as_str().expect("nonce").to_owned()
}

async fn check(
    app: &TestApp,
    token: &str,
    nonce: &str,
    photo: [u8; 3],
    lat: &str,
    long: &str,
) -> (StatusCode, Value) {
    app.call_multipart(
        "/api/v1/attendance",
        Some(token),
        &[
            Part::File("photo", face_jpeg(photo)),
            Part::Text("latitude", lat),
            Part::Text("longitude", long),
            Part::Text("nonce", nonce),
        ],
    )
    .await
}

async fn check_here(f: &Fixture, photo: [u8; 3]) -> (StatusCode, Value) {
    let n = nonce(&f.app, &f.token).await;
    check(&f.app, &f.token, &n, photo, "-6.2003", LONG).await
}

async fn absen_count(f: &Fixture) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM tkabsen WHERE AbsenEmail = ?")
        .bind(&f.email)
        .fetch_one(&f.app.state.db)
        .await
        .expect("count")
}

#[tokio::test]
async fn check_in_then_out_then_refused() {
    let f = fixture().await;

    let (code, body) = check_here(&f, RED).await;
    assert_eq!(code, StatusCode::CREATED, "{body}");
    let data = &body["data"];
    assert_eq!(body["message"], "Absen masuk berhasil");
    assert_eq!(data["attendance_type"], "CHECK IN");
    assert_eq!(data["category"], "masuk");
    assert_eq!(data["shift_uid"], f.shift.as_str());
    assert_eq!(data["shift_time_window"], "00:00 - 23:59");
    assert_eq!(data["location_name"], "Pos Utama");
    assert_eq!(data["radius_meters"], 100);
    assert!(
        data["distance_meters"]
            .as_i64()
            .is_some_and(|d| (30..=40).contains(&d))
    );
    let late = data["late_minutes"].as_i64().expect("late");
    assert_eq!(data["status"], if late > 0 { "late" } else { "on_time" });
    assert_eq!(data["early_leave_minutes"], 0);

    let id: i32 = data["id"]
        .as_str()
        .expect("id")
        .parse()
        .expect("numeric id");
    let row: (String, i32, Option<f32>, Option<String>, i32) = sqlx::query_as(
        "SELECT AbsenType, AFaceVerified, AFaceScore, APhotoPath, ADiffMins FROM tkabsen WHERE AbsenID = ?",
    )
    .bind(id)
    .fetch_one(&f.app.state.db)
    .await
    .expect("absen row");
    assert_eq!(row.0, "CHECK IN");
    assert_eq!(row.1, 1);
    assert!(row.2.is_some_and(|s| s > 0.99));
    assert!(row.3.is_some());
    assert_eq!(i64::from(row.4), late);
    let checkin: Option<String> = sqlx::query_scalar(
        "SELECT ACheckin FROM tkschedule WHERE SFriendEmail = ? AND SGroupUID = ?",
    )
    .bind(&f.email)
    .bind(&f.group)
    .fetch_one(&f.app.state.db)
    .await
    .expect("schedule");
    assert!(checkin.is_some_and(|c| c.len() == 8), "HH:MM:SS");

    let (code, body) = check_here(&f, RED).await;
    assert_eq!(code, StatusCode::CREATED, "{body}");
    assert_eq!(body["message"], "Absen pulang berhasil");
    assert_eq!(body["data"]["attendance_type"], "CHECK OUT");
    assert_eq!(body["data"]["category"], "pulang");
    let early = body["data"]["early_leave_minutes"].as_i64().expect("early");
    assert_eq!(
        body["data"]["status"],
        if early > 0 { "early_leave" } else { "on_time" }
    );

    let (code, body) = check_here(&f, RED).await;
    assert_eq!(code, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error_code"], "ATTENDANCE_ALREADY_COMPLETED");
    assert_eq!(absen_count(&f).await, 2);

    // The new rows show up in the existing list as Masuk and Pulang.
    let (code, body) = f
        .app
        .call("GET", "/api/v1/attendance", Some(&f.token), None)
        .await;
    assert_eq!(code, StatusCode::OK);
    let mut categories: Vec<&str> = body["data"]
        .as_array()
        .expect("list")
        .iter()
        .filter_map(|i| i["category"].as_str())
        .collect();
    // Both rows can share a second, and the list breaks no ties.
    categories.sort_unstable();
    assert_eq!(categories, vec!["masuk", "pulang"]);
}

#[tokio::test]
async fn tolerance_turns_lateness_into_on_time() {
    let f = fixture_with(Some(24 * 60)).await;
    let (code, body) = check_here(&f, RED).await;
    assert_eq!(code, StatusCode::CREATED, "{body}");
    assert_eq!(body["data"]["status"], "on_time");
    assert_eq!(body["data"]["late_minutes"], 0);
    let (code, body) = check_here(&f, RED).await;
    assert_eq!(code, StatusCode::CREATED, "{body}");
    assert_eq!(body["data"]["status"], "on_time");
    assert_eq!(body["data"]["early_leave_minutes"], 0);
}

#[tokio::test]
async fn nonce_is_single_use_and_personal() {
    let f = fixture().await;
    let n = nonce(&f.app, &f.token).await;
    let (code, _) = check(&f.app, &f.token, &n, RED, "-6.2003", LONG).await;
    assert_eq!(code, StatusCode::CREATED);
    let (code, body) = check(&f.app, &f.token, &n, RED, "-6.2003", LONG).await;
    assert_eq!(code, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["error_code"], "ATTENDANCE_NONCE_INVALID");

    let other = f
        .app
        .token_for(&format!("other.{}@test.id", unique()), &f.group, false)
        .await;
    let foreign = nonce(&f.app, &other).await;
    let (code, body) = check(&f.app, &f.token, &foreign, RED, "-6.2003", LONG).await;
    assert_eq!(code, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["error_code"], "ATTENDANCE_NONCE_INVALID");
    assert_eq!(absen_count(&f).await, 1);
}

#[tokio::test]
async fn malformed_requests_are_422() {
    let f = fixture().await;
    let n = nonce(&f.app, &f.token).await;
    for (lat, long) in [("abc", LONG), ("91", LONG), (LAT, "181"), (LAT, "NaN")] {
        let (code, body) = check(&f.app, &f.token, &n, RED, lat, long).await;
        assert_eq!(
            code,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{lat},{long}: {body}"
        );
    }
    let (code, body) = f
        .app
        .call_multipart(
            "/api/v1/attendance",
            Some(&f.token),
            &[
                Part::Text("latitude", LAT),
                Part::Text("longitude", LONG),
                Part::Text("nonce", &n),
            ],
        )
        .await;
    assert_eq!(code, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    let (code, _) = f
        .app
        .call("POST", "/api/v1/attendance", Some(&f.token), None)
        .await;
    assert_eq!(code, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(absen_count(&f).await, 0);
}

#[tokio::test]
async fn outside_the_radius_is_refused() {
    let f = fixture().await;
    let n = nonce(&f.app, &f.token).await;
    let (code, body) = check(&f.app, &f.token, &n, RED, "-6.2100", LONG).await;
    assert_eq!(code, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["error_code"], "ATTENDANCE_OUT_OF_RADIUS");
    assert!(
        body["message"]
            .as_str()
            .is_some_and(|m| m.contains("batas 100 m")),
        "{body}"
    );
    assert_eq!(absen_count(&f).await, 0);
}

#[tokio::test]
async fn zero_radius_means_no_geofence() {
    let f = fixture().await;
    sqlx::query("UPDATE tkshift SET LocRadius = 0 WHERE ShiftUID = ?")
        .bind(&f.shift)
        .execute(&f.app.state.db)
        .await
        .expect("radius 0");
    let n = nonce(&f.app, &f.token).await;
    let (code, body) = check(&f.app, &f.token, &n, RED, "1.0", "100.0").await;
    assert_eq!(code, StatusCode::CREATED, "{body}");
    assert_eq!(body["data"]["distance_meters"], Value::Null);
    assert_eq!(body["data"]["radius_meters"], Value::Null);
}

#[tokio::test]
async fn no_schedule_or_deleted_shift_is_404() {
    let f = fixture().await;
    sqlx::query("UPDATE tkshift SET ShiftDeletedAt = NOW() WHERE ShiftUID = ?")
        .bind(&f.shift)
        .execute(&f.app.state.db)
        .await
        .expect("soft delete");
    let (code, body) = check_here(&f, RED).await;
    assert_eq!(code, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body["error_code"], "ATTENDANCE_NO_SCHEDULE");

    let stranger = f
        .app
        .token_for(&format!("x.{}@test.id", unique()), &f.group, false)
        .await;
    let n = nonce(&f.app, &stranger).await;
    let (code, body) = check(&f.app, &stranger, &n, RED, LAT, LONG).await;
    assert_eq!(code, StatusCode::NOT_FOUND, "{body}");
}

#[tokio::test]
async fn face_must_be_registered_and_match() {
    let f = fixture().await;
    let (code, body) = check_here(&f, GREEN).await;
    assert_eq!(code, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["error_code"], "FACE_NOT_MATCH");
    assert_eq!(body["data"].get("required_percent"), None);
    assert_eq!(body["data"].get("similarity_percent"), None);
    assert_eq!(body["data"]["failed_attempts"], 1);

    let (code, body) = check_here(&f, BLACK).await;
    assert_eq!(code, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(body["error_code"], "FACE_QUALITY_LOW");
    assert_eq!(absen_count(&f).await, 0);

    let (code, _) = f
        .app
        .call("DELETE", "/api/v1/faces/me", Some(&f.token), None)
        .await;
    assert_eq!(code, StatusCode::OK);
    let (code, body) = check_here(&f, RED).await;
    assert_eq!(code, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error_code"], "FACE_NOT_REGISTERED");

    // Registered with another model: verification refuses until re-registration.
    assert_eq!(register(&f.app, &f.token, RED).await.0, StatusCode::CREATED);
    sqlx::query("UPDATE tkfaceembedding SET ModelVersion = 'old' WHERE UserEmail = ?")
        .bind(&f.email)
        .execute(&f.app.state.db)
        .await
        .expect("old model");
    let (code, body) = check_here(&f, RED).await;
    assert_eq!(code, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error_code"], "FACE_NOT_REGISTERED");
    let (_, body) = f
        .app
        .call("GET", "/api/v1/faces/status", Some(&f.token), None)
        .await;
    assert_eq!(body["data"]["needs_reregistration"], true);
}

#[tokio::test]
async fn attendance_photo_is_for_group_admins_only() {
    let f = fixture().await;
    let (code, body) = check_here(&f, RED).await;
    assert_eq!(code, StatusCode::CREATED, "{body}");
    assert_eq!(body["data"].get("similarity_percent"), None);
    let url = format!(
        "/api/v1/attendance/{}/photo",
        body["data"]["id"].as_str().expect("id")
    );

    let (code, _, _) = f.app.call_raw("GET", &url, Some(&f.token)).await;
    assert_eq!(code, StatusCode::FORBIDDEN);

    let tag = unique();
    let outsider = f
        .app
        .token_for(&format!("adm.{tag}@test.id"), &format!("x{tag}"), true)
        .await;
    let (code, _, _) = f.app.call_raw("GET", &url, Some(&outsider)).await;
    assert_eq!(code, StatusCode::NOT_FOUND);

    let admin = f
        .app
        .token_for(&format!("admin.{tag}@test.id"), &f.group, true)
        .await;
    let (code, headers, bytes) = f.app.call_raw("GET", &url, Some(&admin)).await;
    assert_eq!(code, StatusCode::OK);
    assert_eq!(headers[header::CONTENT_TYPE], "image/jpeg");
    assert!(bytes.starts_with(&[0xFF, 0xD8]));
    let audits: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM tkfaceaudit WHERE Action = 'VIEW_ATTENDANCE_PHOTO' AND TargetEmail = ?",
    )
    .bind(&f.email)
    .fetch_one(&f.app.state.db)
    .await
    .expect("audit");
    assert_eq!(audits, 1);

    let (code, _, _) = f
        .app
        .call_raw("GET", "/api/v1/attendance/abc/photo", Some(&admin))
        .await;
    assert_eq!(code, StatusCode::UNPROCESSABLE_ENTITY);
    let (code, _, _) = f
        .app
        .call_raw("GET", "/api/v1/attendance/999999999/photo", Some(&admin))
        .await;
    assert_eq!(code, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn stats_count_the_period() {
    let f = fixture().await;
    assert_eq!(check_here(&f, RED).await.0, StatusCode::CREATED);
    assert_eq!(check_here(&f, RED).await.0, StatusCode::CREATED);
    let today = Local::now().date_naive().format("%Y-%m-%d").to_string();

    let path = format!(
        "/api/v1/attendance/stats?from={today}&to={today}&email={}",
        f.email
    );
    let (code, body) = f.app.call("GET", &path, Some(&f.token), None).await;
    assert_eq!(code, StatusCode::OK, "{body}");
    let data = &body["data"];
    assert_eq!(data["from"], today.as_str());
    assert_eq!(data["total"], 2);
    assert_eq!(data["check_in"], 1);
    assert_eq!(data["check_out"], 1);
    assert_eq!(data["other"], 0);
    assert!(data["late"].as_u64().is_some_and(|n| n <= 1));
    assert!(data["early_leave"].as_u64().is_some_and(|n| n <= 1));

    let (code, body) = f
        .app
        .call(
            "GET",
            "/api/v1/attendance/stats?from=2020-01-01&to=2020-01-31",
            Some(&f.token),
            None,
        )
        .await;
    assert_eq!(code, StatusCode::OK);
    assert_eq!(body["data"]["total"], 0);

    let (code, _) = f
        .app
        .call(
            "GET",
            "/api/v1/attendance/stats?from=2026-01-01&to=2026-06-01",
            Some(&f.token),
            None,
        )
        .await;
    assert_eq!(code, StatusCode::UNPROCESSABLE_ENTITY);
}
