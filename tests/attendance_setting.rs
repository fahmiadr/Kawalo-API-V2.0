//! `GET`/`PUT /attendance/settings` and the group's threshold in `POST /attendance`, with the fake
//! face engine. Each test uses its own group and emails.
//!
//! Shifts run 00:00-23:59 today (see `attendance_write.rs`).

mod common;

use axum::http::StatusCode;
use chrono::{Datelike, Local};
use common::faces::{self, RED, RED_TILTED, face_jpeg, join, register, token_with_role, unique};
use common::{Part, TestApp};
use serde_json::{Value, json};
use tempfile::TempDir;

const SETTINGS: &str = "/api/v1/attendance/settings";
const LONG: &str = "106.8000000";

struct Fixture {
    app: TestApp,
    _photos: TempDir,
    group: String,
    guard: String,
    admin: String,
}

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
    sqlx::query(
        "INSERT INTO tkshift (ShiftName, ShiftTimeStart, ShiftTimeStart_mm, ShiftTimeStop, \
         ShiftTimeStop_mm, ShiftGroupUID, ShiftColor, ShiftUID, LocName, LocLat, LocLong, LocRadius) \
         VALUES ('Shift Uji', 0, 0, 23, 59, ?, '#112233', ?, 'Pos Utama', '-6.2000000', ?, 100)",
    )
    .bind(&group)
    .bind(&shift)
    .bind(LONG)
    .execute(&app.state.db)
    .await
    .expect("insert shift");
    let today = Local::now().date_naive();
    sqlx::query(
        "INSERT INTO tkschedule (SchedShiftUID, SDate, SMonth, SYear, SFriendEmail, SFriendName, \
         SGroupUID) VALUES (?, ?, ?, ?, ?, 'Nama', ?)",
    )
    .bind(&shift)
    .bind(i32::try_from(today.day()).expect("day"))
    .bind(i32::try_from(today.month()).expect("month"))
    .bind(today.year())
    .bind(&email)
    .bind(&group)
    .execute(&app.state.db)
    .await
    .expect("insert schedule");
    let guard = app.token_for(&email, &group, false).await;
    assert_eq!(register(&app, &guard, RED).await.0, StatusCode::CREATED);
    let admin = app
        .token_for(&format!("admin.{tag}@test.id"), &group, true)
        .await;
    Fixture {
        app,
        _photos: photos,
        group,
        guard,
        admin,
    }
}

async fn put(f: &Fixture, token: &str, value: Value) -> (StatusCode, Value) {
    f.app
        .call(
            "PUT",
            SETTINGS,
            Some(token),
            Some(&json!({"face_match_percent": value})),
        )
        .await
}

async fn get(f: &Fixture, token: &str) -> Value {
    let (code, body) = f.app.call("GET", SETTINGS, Some(token), None).await;
    assert_eq!(code, StatusCode::OK, "{body}");
    body["data"].clone()
}

async fn check(f: &Fixture, photo: [u8; 3]) -> (StatusCode, Value) {
    let (code, body) = f
        .app
        .call("POST", "/api/v1/attendance/nonce", Some(&f.guard), None)
        .await;
    assert_eq!(code, StatusCode::CREATED, "{body}");
    f.app
        .call_multipart(
            "/api/v1/attendance",
            Some(&f.guard),
            &[
                Part::File("photo", face_jpeg(photo)),
                Part::Text("latitude", "-6.2003"),
                Part::Text("longitude", LONG),
                Part::Text("nonce", body["data"]["nonce"].as_str().expect("nonce")),
            ],
        )
        .await
}

#[tokio::test]
async fn defaults_then_admin_sets_and_resets() {
    let f = fixture().await;
    let data = get(&f, &f.guard).await;
    assert_eq!(data["face_match_percent"], 50);
    assert_eq!(data["default_percent"], 50);
    assert_eq!(data["min_percent"], 20);
    assert_eq!(data["max_percent"], 95);
    assert_eq!(data["is_default"], true);
    assert_eq!(data["updated_by"], Value::Null);
    assert_eq!(data["updated_at"], Value::Null);

    let (code, body) = put(&f, &f.admin, json!(75)).await;
    assert_eq!(code, StatusCode::OK, "{body}");
    assert_eq!(body["message"], "Pengaturan absensi berhasil disimpan");
    assert_eq!(body["data"]["face_match_percent"], 75);
    assert_eq!(body["data"]["is_default"], false);
    assert!(
        body["data"]["updated_by"]
            .as_str()
            .is_some_and(|e| e.starts_with("admin."))
    );
    assert!(body["data"]["updated_at"].is_string());
    assert_eq!(get(&f, &f.guard).await["face_match_percent"], 75);

    let (code, body) = put(&f, &f.admin, json!(20)).await;
    assert_eq!(code, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["face_match_percent"], 20);

    let (code, body) = put(&f, &f.admin, Value::Null).await;
    assert_eq!(code, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["face_match_percent"], 50);
    assert_eq!(body["data"]["is_default"], true);
    assert_eq!(body["data"]["updated_by"], Value::Null);
}

#[tokio::test]
async fn only_admin_roles_write_and_input_is_validated() {
    let f = fixture().await;
    let (code, _) = put(&f, &f.guard, json!(70)).await;
    assert_eq!(code, StatusCode::FORBIDDEN);
    // allow_admin on a non-admin role does not count.
    let flagged = token_with_role(&f.app, "flag@test.id", &f.group, "GUARD").await;
    let (code, _) = put(&f, &flagged, json!(70)).await;
    assert_eq!(code, StatusCode::FORBIDDEN);

    for bad in [json!(19), json!(96), json!(70.5), json!("70")] {
        let (code, body) = put(&f, &f.admin, bad.clone()).await;
        assert_eq!(code, StatusCode::UNPROCESSABLE_ENTITY, "{bad}: {body}");
    }
    let (_, body) = put(&f, &f.admin, json!(19)).await;
    assert!(
        body["message"]
            .as_str()
            .is_some_and(|m| m.contains("20") && m.contains("95")),
        "{body}"
    );
    assert_eq!(get(&f, &f.guard).await["is_default"], true);

    // Validation runs before auth: a bad body without a token is 422, a good one 401.
    let (code, _) = f
        .app
        .call(
            "PUT",
            SETTINGS,
            None,
            Some(&json!({"face_match_percent": 5})),
        )
        .await;
    assert_eq!(code, StatusCode::UNPROCESSABLE_ENTITY);
    let (code, _) = f
        .app
        .call(
            "PUT",
            SETTINGS,
            None,
            Some(&json!({"face_match_percent": 50})),
        )
        .await;
    assert_eq!(code, StatusCode::UNAUTHORIZED);
    let (code, _) = f.app.call("GET", SETTINGS, None, None).await;
    assert_eq!(code, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn settings_are_scoped_to_the_active_group() {
    let f = fixture().await;
    assert_eq!(put(&f, &f.admin, json!(90)).await.0, StatusCode::OK);

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
        assert_eq!(get(&f, token).await["is_default"], true);
        let (code, body) = put(&f, token, json!(30)).await;
        assert_eq!(code, StatusCode::OK, "{body}");
        assert_eq!(body["data"]["face_match_percent"], 30);
    }
    let data = get(&f, &f.admin).await;
    assert_eq!(data["face_match_percent"], 90);
    assert!(
        data["updated_by"]
            .as_str()
            .is_some_and(|e| e.starts_with("admin."))
    );
}

#[tokio::test]
async fn check_in_uses_the_group_threshold_and_hides_the_score() {
    let f = fixture().await;

    // RED_TILTED vs RED is about 71%.
    assert_eq!(put(&f, &f.admin, json!(90)).await.0, StatusCode::OK);
    let (code, body) = check(&f, RED_TILTED).await;
    assert_eq!(code, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["error_code"], "FACE_NOT_MATCH");
    assert_eq!(body["data"]["failed_attempts"], 1);
    assert_eq!(body["data"].get("required_percent"), None);
    assert_eq!(body["data"].get("similarity_percent"), None);

    assert_eq!(put(&f, &f.admin, json!(60)).await.0, StatusCode::OK);
    let (code, body) = check(&f, RED_TILTED).await;
    assert_eq!(code, StatusCode::CREATED, "{body}");
    assert_eq!(body["data"].get("similarity_percent"), None);

    // The admin sees the raw stored score; everyone else gets null.
    let list = |token: String| {
        let f = &f;
        async move {
            let (code, body) = f
                .app
                .call("GET", "/api/v1/attendance", Some(&token), None)
                .await;
            assert_eq!(code, StatusCode::OK, "{body}");
            body["data"][0]["face_similarity_percent"].clone()
        }
    };
    let seen = list(f.admin.clone())
        .await
        .as_i64()
        .expect("admin sees a percent");
    assert!((65..=77).contains(&seen), "{seen}");
    assert_eq!(list(f.guard.clone()).await, Value::Null);
    // allow_admin without an admin role does not reveal it.
    let flagged = token_with_role(&f.app, "flag2@test.id", &f.group, "GUARD").await;
    assert_eq!(list(flagged).await, Value::Null);
    let superadmin = token_with_role(&f.app, "sa2@test.id", &f.group, "SUPER_ADMIN").await;
    assert_eq!(list(superadmin).await.as_i64(), Some(seen));

    // Back to the default (50%): the check-out passes with the same face.
    assert_eq!(put(&f, &f.admin, Value::Null).await.0, StatusCode::OK);
    let (code, body) = check(&f, RED_TILTED).await;
    assert_eq!(code, StatusCode::CREATED, "{body}");
    assert_eq!(body["message"], "Absen pulang berhasil");
}
