//! Face-failure counting and manual attendance requests against the scratch services with the
//! fake face engine. Each test uses its own group and emails.
//!
//! Shifts run 00:00-23:59 today, so a check-in is late and a check-out early at any time of day.
//! A run across local midnight can fail spuriously.

mod common;

use axum::http::StatusCode;
use chrono::{Datelike, Local};
use common::faces::{self, GREEN, RED, face_jpeg, join, register, token_with_role, unique};
use common::{Part, TestApp};
use serde_json::{Value, json};
use tempfile::TempDir;

const LAT: &str = "-6.2000000";
const LONG: &str = "106.8000000";
const REQUESTS: &str = "/api/v1/attendance/manual-requests";

struct Fixture {
    app: TestApp,
    _photos: TempDir,
    group: String,
    email: String,
    token: String,
    admin: String,
    shift: String,
}

async fn insert_shift(app: &TestApp, group: &str, uid: &str) {
    sqlx::query(
        "INSERT INTO tkshift (ShiftName, ShiftTimeStart, ShiftTimeStart_mm, ShiftTimeStop, \
         ShiftTimeStop_mm, ShiftGroupUID, ShiftColor, ShiftUID, LocName, LocLat, LocLong, LocRadius) \
         VALUES ('Shift Uji', 0, 0, 23, 59, ?, '#112233', ?, 'Pos Utama', ?, ?, 100)",
    )
    .bind(group)
    .bind(uid)
    .bind(LAT)
    .bind(LONG)
    .execute(&app.state.db)
    .await
    .expect("insert shift");
}

async fn insert_today(app: &TestApp, group: &str, email: &str, shift: &str) {
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
    .expect("insert schedule");
}

/// A guard with a registered face (red), a 100 m geofenced shift and a schedule today, plus an
/// admin of the same group.
async fn fixture() -> Fixture {
    let photos = tempfile::tempdir().expect("photo dir");
    let dir = photos.path().to_owned();
    let app = TestApp::start_with_state(
        |config| faces::configure(config, &dir),
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
    insert_shift(&app, &group, &shift).await;
    insert_today(&app, &group, &email, &shift).await;
    let token = app.token_for(&email, &group, false).await;
    assert_eq!(register(&app, &token, RED).await.0, StatusCode::CREATED);
    let admin = app
        .token_for(&format!("admin.{tag}@test.id"), &group, true)
        .await;
    Fixture {
        app,
        _photos: photos,
        group,
        email,
        token,
        admin,
        shift,
    }
}

async fn nonce(app: &TestApp, token: &str) -> Value {
    let (code, body) = app
        .call("POST", "/api/v1/attendance/nonce", Some(token), None)
        .await;
    assert_eq!(code, StatusCode::CREATED, "{body}");
    body["data"].clone()
}

async fn check(f: &Fixture, photo: [u8; 3]) -> (StatusCode, Value) {
    let n = nonce(&f.app, &f.token).await;
    f.app
        .call_multipart(
            "/api/v1/attendance",
            Some(&f.token),
            &[
                Part::File("photo", face_jpeg(photo)),
                Part::Text("latitude", "-6.2003"),
                Part::Text("longitude", LONG),
                Part::Text("nonce", n["nonce"].as_str().expect("nonce")),
            ],
        )
        .await
}

async fn fail_three_times(f: &Fixture) {
    for _ in 0..3 {
        let (code, _) = check(f, GREEN).await;
        assert_eq!(code, StatusCode::FORBIDDEN);
    }
}

fn request_body(reason: &str) -> Value {
    json!({"reason": reason, "note": "Kamera rusak", "latitude": -6.2003, "longitude": 106.8})
}

async fn file(f: &Fixture, reason: &str) -> (StatusCode, Value) {
    f.app
        .call(
            "POST",
            REQUESTS,
            Some(&f.token),
            Some(&request_body(reason)),
        )
        .await
}

async fn decide(f: &Fixture, id: &str, action: &str) -> (StatusCode, Value) {
    f.app
        .call(
            "PATCH",
            &format!("{REQUESTS}/{id}"),
            Some(&f.admin),
            Some(&json!({"action": action, "note": "Diperiksa"})),
        )
        .await
}

async fn absen_rows(f: &Fixture) -> Vec<(String, i32, Option<String>, i32)> {
    sqlx::query_as(
        "SELECT AbsenType, AFaceVerified, APhotoPath, AEarlyMins FROM tkabsen \
         WHERE AbsenEmail = ? ORDER BY AbsenID",
    )
    .bind(&f.email)
    .fetch_all(&f.app.state.db)
    .await
    .expect("rows")
}

#[tokio::test]
async fn mismatches_are_counted_per_day_and_cleared_by_a_success() {
    let f = fixture().await;
    assert_eq!(nonce(&f.app, &f.token).await["failed_attempts"], 0);
    for (n, available) in [(1, false), (2, false), (3, true)] {
        let (code, body) = check(&f, GREEN).await;
        assert_eq!(code, StatusCode::FORBIDDEN, "{body}");
        assert_eq!(body["error_code"], "FACE_NOT_MATCH");
        assert_eq!(body["data"]["failed_attempts"], n, "{body}");
        assert_eq!(body["data"]["max_attempts"], 3);
        assert_eq!(body["data"]["manual_request_available"], available);
    }
    let issued = nonce(&f.app, &f.token).await;
    assert_eq!(issued["failed_attempts"], 3);
    assert_eq!(issued["manual_request_available"], true);

    // A photo-quality failure is not a mismatch.
    let (code, body) = check(&f, [0, 0, 0]).await;
    assert_eq!(code, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(nonce(&f.app, &f.token).await["failed_attempts"], 3);

    assert_eq!(check(&f, RED).await.0, StatusCode::CREATED);
    assert_eq!(nonce(&f.app, &f.token).await["failed_attempts"], 0);
}

/// Three mismatches only unlock "Lapor ke admin"; they never block further attempts.
#[tokio::test]
async fn attempts_are_never_blocked_after_three_mismatches() {
    let f = fixture().await;
    fail_three_times(&f).await;
    let (code, body) = check(&f, GREEN).await;
    assert_eq!(code, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["error_code"], "FACE_NOT_MATCH");
    assert_eq!(body["data"]["failed_attempts"], 4);
    let (code, body) = check(&f, RED).await;
    assert_eq!(
        code,
        StatusCode::CREATED,
        "fifth attempt with the right face: {body}"
    );
    assert_eq!(nonce(&f.app, &f.token).await["failed_attempts"], 0);
}

#[tokio::test]
async fn face_failed_needs_three_mismatches_but_camera_does_not() {
    let f = fixture().await;
    let (code, body) = file(&f, "face_failed").await;
    assert_eq!(code, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["error_code"], "ATTENDANCE_MANUAL_UNAVAILABLE");
    assert_eq!(body["data"]["failed_attempts"], 0);

    fail_three_times(&f).await;
    let (code, body) = file(&f, "face_failed").await;
    assert_eq!(code, StatusCode::CREATED, "{body}");
    let data = &body["data"];
    assert_eq!(data["status"], "PENDING");
    assert_eq!(data["reason"], "face_failed");
    assert_eq!(data["note"], "Kamera rusak");
    assert_eq!(data["attendance_type"], "CHECK IN");
    assert_eq!(data["category"], "masuk");
    assert_eq!(data["user_email"], f.email.as_str());
    assert_eq!(data["shift_uid"], f.shift.as_str());
    assert_eq!(data["shift_time_window"], "00:00 - 23:59");
    assert_eq!(data["location_name"], "Pos Utama");
    assert_eq!(data["failed_attempts"], 3);
    assert_eq!(data["decided_by"], Value::Null);
    assert_eq!(data["attendance_id"], Value::Null);
    assert!(absen_rows(&f).await.is_empty());

    let g = fixture().await;
    let (code, body) = file(&g, "camera_unavailable").await;
    assert_eq!(code, StatusCode::CREATED, "{body}");
    assert_eq!(body["data"]["failed_attempts"], 0);
}

#[tokio::test]
async fn request_rules_match_a_normal_attendance() {
    let f = fixture().await;

    let far = json!({"reason": "camera_unavailable", "latitude": -6.21, "longitude": 106.8});
    let (code, body) = f
        .app
        .call("POST", REQUESTS, Some(&f.token), Some(&far))
        .await;
    assert_eq!(code, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["error_code"], "ATTENDANCE_OUT_OF_RADIUS");

    for bad in [
        json!({"reason": "other", "latitude": 1, "longitude": 1}),
        json!({"reason": "camera_unavailable", "latitude": 91, "longitude": 1}),
        json!({"reason": "camera_unavailable", "latitude": 1}),
        json!({"reason": "camera_unavailable", "note": "x".repeat(201), "latitude": 1, "longitude": 1}),
    ] {
        let (code, body) = f
            .app
            .call("POST", REQUESTS, Some(&f.token), Some(&bad))
            .await;
        assert_eq!(code, StatusCode::UNPROCESSABLE_ENTITY, "{bad}: {body}");
    }
    // The body is validated before the caller, as everywhere else.
    let (code, _) = f
        .app
        .call("POST", REQUESTS, None, Some(&json!({"reason": "x"})))
        .await;
    assert_eq!(code, StatusCode::UNPROCESSABLE_ENTITY);
    let (code, _) = f
        .app
        .call(
            "POST",
            REQUESTS,
            None,
            Some(&request_body("camera_unavailable")),
        )
        .await;
    assert_eq!(code, StatusCode::UNAUTHORIZED);

    let stranger = f
        .app
        .token_for(&format!("x.{}@test.id", unique()), &f.group, false)
        .await;
    let (code, body) = f
        .app
        .call(
            "POST",
            REQUESTS,
            Some(&stranger),
            Some(&request_body("camera_unavailable")),
        )
        .await;
    assert_eq!(code, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body["error_code"], "ATTENDANCE_NO_SCHEDULE");

    let (code, _) = file(&f, "camera_unavailable").await;
    assert_eq!(code, StatusCode::CREATED);
    let (code, body) = file(&f, "camera_unavailable").await;
    assert_eq!(code, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error_code"], "CONFLICT_ERROR");
}

#[tokio::test]
async fn admins_of_the_group_list_and_filter() {
    let f = fixture().await;
    let (_, created) = file(&f, "camera_unavailable").await;
    let id = created["data"]["id"].as_str().expect("id").to_owned();

    let (code, _) = f.app.call("GET", REQUESTS, Some(&f.token), None).await;
    assert_eq!(code, StatusCode::FORBIDDEN);
    let (code, _) = f.app.call("GET", REQUESTS, None, None).await;
    assert_eq!(code, StatusCode::UNAUTHORIZED);

    let (code, body) = f.app.call("GET", REQUESTS, Some(&f.admin), None).await;
    assert_eq!(code, StatusCode::OK, "{body}");
    assert_eq!(body["data"].as_array().expect("list").len(), 1);
    assert_eq!(body["data"][0]["id"], id.as_str());
    assert_eq!(body["data"][0]["avatar_url"], Value::Null, "no tkuser row");
    sqlx::query(
        "INSERT INTO tkuser (UserUID, UserEmail, UserName, Password, UserProfilePic) \
         VALUES (?, ?, 'Satpam', 'x', 'https://cdn.test/g.webp')",
    )
    .bind(unique())
    .bind(&f.email)
    .execute(&f.app.state.db)
    .await
    .expect("insert user");
    let (_, body) = f.app.call("GET", REQUESTS, Some(&f.admin), None).await;
    assert_eq!(body["data"][0]["avatar_url"], "https://cdn.test/g.webp");
    for (status, count) in [("PENDING", 1), ("APPROVED", 0), ("REJECTED", 0)] {
        let (_, body) = f
            .app
            .call(
                "GET",
                &format!("{REQUESTS}?status={status}&limit=10&offset=0"),
                Some(&f.admin),
                None,
            )
            .await;
        assert_eq!(
            body["data"].as_array().expect("list").len(),
            count,
            "{status}"
        );
    }
    let (code, _) = f
        .app
        .call(
            "GET",
            &format!("{REQUESTS}?status=DONE"),
            Some(&f.admin),
            None,
        )
        .await;
    assert_eq!(code, StatusCode::UNPROCESSABLE_ENTITY);

    // Another group's admin, SUPER_ADMIN included, sees and decides nothing here.
    let tag = unique();
    let outsider = f
        .app
        .token_for(&format!("o.{tag}@test.id"), &format!("x{tag}"), true)
        .await;
    let superadmin = token_with_role(
        &f.app,
        &format!("sa.{tag}@test.id"),
        &format!("y{tag}"),
        "SUPER_ADMIN",
    )
    .await;
    for token in [&outsider, &superadmin] {
        let (_, body) = f.app.call("GET", REQUESTS, Some(token), None).await;
        assert_eq!(body["data"], json!([]));
        let (code, _) = f
            .app
            .call(
                "PATCH",
                &format!("{REQUESTS}/{id}"),
                Some(token),
                Some(&json!({"action": "approve"})),
            )
            .await;
        assert_eq!(code, StatusCode::NOT_FOUND);
    }
    assert!(absen_rows(&f).await.is_empty());
}

#[tokio::test]
async fn approving_writes_an_unverified_row_once() {
    let f = fixture().await;
    fail_three_times(&f).await;
    let (_, created) = file(&f, "face_failed").await;
    let id = created["data"]["id"].as_str().expect("id").to_owned();
    let requested_at = created["data"]["requested_at"]
        .as_str()
        .expect("time")
        .to_owned();

    // A guard cannot decide, and a bad action is a 422.
    let (code, _) = f
        .app
        .call(
            "PATCH",
            &format!("{REQUESTS}/{id}"),
            Some(&f.token),
            Some(&json!({"action": "approve"})),
        )
        .await;
    assert_eq!(code, StatusCode::FORBIDDEN);
    let (code, _) = decide(&f, &id, "maybe").await;
    assert_eq!(code, StatusCode::UNPROCESSABLE_ENTITY);

    let (code, body) = decide(&f, &id, "approve").await;
    assert_eq!(code, StatusCode::OK, "{body}");
    let data = &body["data"];
    assert_eq!(data["status"], "APPROVED");
    assert_eq!(data["decision_note"], "Diperiksa");
    assert!(
        data["decided_by"]
            .as_str()
            .is_some_and(|by| by.starts_with("admin."))
    );
    assert!(data["decided_at"].is_string());
    let attendance_id = data["attendance_id"]
        .as_str()
        .expect("attendance id")
        .to_owned();

    let rows = absen_rows(&f).await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].0, "CHECK IN");
    assert_eq!(rows[0].1, 0, "unverified");
    assert_eq!(rows[0].2, None, "no photo");
    let taken_at: String =
        sqlx::query_scalar("SELECT CAST(AbsenDT AS CHAR) FROM tkabsen WHERE AbsenID = ?")
            .bind(attendance_id.parse::<i32>().expect("id"))
            .fetch_one(&f.app.state.db)
            .await
            .expect("time");
    assert_eq!(
        taken_at.replace(' ', "T")[..19],
        requested_at[..19],
        "request time"
    );
    let (checkin, late): (Option<String>, i32) = sqlx::query_as(
        "SELECT ACheckin, LateMins FROM tkschedule WHERE SFriendEmail = ? AND SGroupUID = ?",
    )
    .bind(&f.email)
    .bind(&f.group)
    .fetch_one(&f.app.state.db)
    .await
    .expect("schedule");
    assert!(checkin.is_some_and(|c| c.len() == 8));
    assert!(late > 0, "00:00 shift, so the request was late");

    // Decided once only.
    for action in ["approve", "reject"] {
        let (code, body) = decide(&f, &id, action).await;
        assert_eq!(code, StatusCode::CONFLICT, "{body}");
    }
    assert_eq!(absen_rows(&f).await.len(), 1);

    // The counter restarts and the list marks the row unverified, without a photo.
    assert_eq!(nonce(&f.app, &f.token).await["failed_attempts"], 0);
    let (_, list) = f
        .app
        .call("GET", "/api/v1/attendance", Some(&f.token), None)
        .await;
    let item = &list["data"][0];
    assert_eq!(item["id"], attendance_id.as_str());
    assert_eq!(item["face_verified"], false);
    assert_eq!(item["has_photo"], false);
    assert_eq!(item["face_similarity_percent"], Value::Null);
    let (_, as_admin) = f
        .app
        .call("GET", "/api/v1/attendance", Some(&f.admin), None)
        .await;
    assert_eq!(as_admin["data"][0]["id"], attendance_id.as_str());
    assert_eq!(as_admin["data"][0]["face_similarity_percent"], Value::Null);

    // The shift is open now: the next manual request is the check-out.
    let (code, body) = file(&f, "camera_unavailable").await;
    assert_eq!(code, StatusCode::CREATED, "{body}");
    assert_eq!(body["data"]["attendance_type"], "CHECK OUT");
    let (code, body) = decide(&f, body["data"]["id"].as_str().expect("id"), "approve").await;
    assert_eq!(code, StatusCode::OK, "{body}");
    let rows = absen_rows(&f).await;
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[1].0, "CHECK OUT");
    assert!(
        rows[1].3 > 0,
        "00:00-23:59 shift, so the check-out was early"
    );

    let (code, body) = file(&f, "camera_unavailable").await;
    assert_eq!(code, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error_code"], "ATTENDANCE_ALREADY_COMPLETED");
}

#[tokio::test]
async fn rejecting_writes_nothing_and_frees_the_day() {
    let f = fixture().await;
    let (_, created) = file(&f, "camera_unavailable").await;
    let id = created["data"]["id"].as_str().expect("id").to_owned();
    let (code, body) = decide(&f, &id, "reject").await;
    assert_eq!(code, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["status"], "REJECTED");
    assert_eq!(body["data"]["attendance_id"], Value::Null);
    assert!(absen_rows(&f).await.is_empty());

    let (code, body) = file(&f, "camera_unavailable").await;
    assert_eq!(code, StatusCode::CREATED, "{body}");
    let (_, body) = f
        .app
        .call(
            "GET",
            &format!("{REQUESTS}?status=REJECTED"),
            Some(&f.admin),
            None,
        )
        .await;
    assert_eq!(body["data"].as_array().expect("list").len(), 1);
}

#[tokio::test]
async fn a_request_gone_stale_cannot_be_approved() {
    let f = fixture().await;
    let (_, created) = file(&f, "camera_unavailable").await;
    let id = created["data"]["id"].as_str().expect("id").to_owned();

    // The guard's face works after all and checks in normally.
    assert_eq!(check(&f, RED).await.0, StatusCode::CREATED);
    let (code, body) = decide(&f, &id, "approve").await;
    assert_eq!(code, StatusCode::CONFLICT, "{body}");
    assert_eq!(absen_rows(&f).await.len(), 1);
    let status: String =
        sqlx::query_scalar("SELECT Status FROM tkattendancemanual WHERE ManualId = ?")
            .bind(id.parse::<i32>().expect("id"))
            .fetch_one(&f.app.state.db)
            .await
            .expect("status");
    assert_eq!(status, "PENDING", "the failed approval rolled back");

    let (code, _) = decide(&f, &id, "reject").await;
    assert_eq!(code, StatusCode::OK);
}

#[tokio::test]
async fn list_marks_verification_and_photos() {
    let f = fixture().await;
    assert_eq!(check(&f, RED).await.0, StatusCode::CREATED);
    sqlx::query(
        "INSERT INTO tkabsen (AbsenEmail, AbsenName, AbsenDT, AbsenType, AbsenGroupUID) \
         VALUES (?, 'Lama', NOW() - INTERVAL 1 DAY, 'CHECK IN', ?)",
    )
    .bind(&f.email)
    .bind(&f.group)
    .execute(&f.app.state.db)
    .await
    .expect("legacy row");

    let (code, list) = f
        .app
        .call("GET", "/api/v1/attendance", Some(&f.token), None)
        .await;
    assert_eq!(code, StatusCode::OK, "{list}");
    let items = list["data"].as_array().expect("items");
    assert_eq!(items.len(), 2);
    let verified = items
        .iter()
        .find(|i| i["face_verified"] == true)
        .expect("verified row");
    assert_eq!(verified["has_photo"], true);
    let legacy = items
        .iter()
        .find(|i| i["user_name"] == "Lama")
        .expect("legacy row");
    assert_eq!(legacy["face_verified"], Value::Null);
    assert_eq!(legacy["has_photo"], false);
    assert_eq!(legacy["early_leave_minutes"], 0);
}
