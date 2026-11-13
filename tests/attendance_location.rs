//! `GET`/`PUT /attendance/location` and the group's point in `POST /attendance` and manual
//! requests, with the fake face engine. Each test uses its own group and emails.
//!
//! The shift's own (legacy) location is -6.2 / `LONG` radius 100 ("Pos Utama"); shifts run
//! 00:00-23:59 today (see `attendance_write.rs`).

mod common;

use axum::http::StatusCode;
use chrono::{Datelike, Local};
use common::faces::{self, RED, face_jpeg, join, register, token_with_role, unique};
use common::{Part, TestApp};
use serde_json::{Value, json};
use tempfile::TempDir;

const LOCATION: &str = "/api/v1/attendance/location";
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

fn point(lat: f64, radius: i64) -> Value {
    json!({"location_name": "  Gerbang Barat ", "glat": lat, "glong": 106.8, "radius": radius})
}

async fn put(f: &Fixture, token: Option<&str>, body: Value) -> (StatusCode, Value) {
    f.app.call("PUT", LOCATION, token, Some(&body)).await
}

async fn get(f: &Fixture, token: &str) -> Value {
    let (code, body) = f.app.call("GET", LOCATION, Some(token), None).await;
    assert_eq!(code, StatusCode::OK, "{body}");
    body["data"].clone()
}

async fn check(f: &Fixture) -> (StatusCode, Value) {
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
                Part::File("photo", face_jpeg(RED)),
                Part::Text("latitude", "-6.2003"),
                Part::Text("longitude", LONG),
                Part::Text("nonce", body["data"]["nonce"].as_str().expect("nonce")),
            ],
        )
        .await
}

#[tokio::test]
async fn unset_then_admin_sets_and_replaces() {
    let f = fixture().await;
    let data = get(&f, &f.guard).await;
    assert_eq!(
        data,
        json!({
            "configured": false, "location_name": null, "glat": null, "glong": null,
            "radius": null, "min_radius": 10, "max_radius": 1000,
            "updated_by": null, "updated_at": null,
        })
    );

    let (code, body) = put(&f, Some(&f.admin), point(-6.25, 80)).await;
    assert_eq!(code, StatusCode::OK, "{body}");
    assert_eq!(body["message"], "Titik absen berhasil disimpan");
    let data = &body["data"];
    assert_eq!(data["configured"], true);
    assert_eq!(data["location_name"], "Gerbang Barat");
    assert_eq!(
        (&data["glat"], &data["glong"]),
        (&json!(-6.25), &json!(106.8))
    );
    assert_eq!(data["radius"], 80);
    assert!(
        data["updated_by"]
            .as_str()
            .is_some_and(|e| e.starts_with("admin."))
    );
    assert!(data["updated_at"].is_string());
    assert_eq!(get(&f, &f.guard).await["radius"], 80);

    // A blank name is stored as "N/A" and read back as null; PUT replaces the previous point.
    let mut next = point(-6.3, 10);
    next["location_name"] = json!("   ");
    let (code, body) = put(&f, Some(&f.admin), next).await;
    assert_eq!(code, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["location_name"], Value::Null);
    assert_eq!(body["data"]["glat"], -6.3);
    assert_eq!(body["data"]["radius"], 10);
    let mut no_name = point(-6.3, 1000);
    no_name
        .as_object_mut()
        .expect("object")
        .remove("location_name");
    assert_eq!(put(&f, Some(&f.admin), no_name).await.0, StatusCode::OK);
}

#[tokio::test]
async fn only_admin_roles_write_and_input_is_validated() {
    let f = fixture().await;
    let (code, _) = put(&f, Some(&f.guard), point(-6.25, 80)).await;
    assert_eq!(code, StatusCode::FORBIDDEN);
    // allow_admin on a non-admin role does not count.
    let flagged = token_with_role(&f.app, "flag@test.id", &f.group, "GUARD").await;
    let (code, _) = put(&f, Some(&flagged), point(-6.25, 80)).await;
    assert_eq!(code, StatusCode::FORBIDDEN);

    let bad_bodies = [
        point(-6.25, 9),
        point(-6.25, 1001),
        point(-6.25, 0),
        json!({"glat": -6.25, "glong": 106.8, "radius": 50.5}),
        json!({"glat": -6.25, "glong": 106.8, "radius": "50"}),
        json!({"glat": -6.25, "glong": 106.8}),
        json!({"glat": -6.25, "radius": 50}),
        json!({"glong": 106.8, "radius": 50}),
        json!({"glat": 91, "glong": 106.8, "radius": 50}),
        json!({"glat": -6.25, "glong": 181, "radius": 50}),
        json!({"glat": 0, "glong": 0, "radius": 50}),
        json!({"location_name": "x".repeat(129), "glat": 1, "glong": 1, "radius": 50}),
    ];
    for bad in bad_bodies {
        let (code, body) = put(&f, Some(&f.admin), bad.clone()).await;
        assert_eq!(code, StatusCode::UNPROCESSABLE_ENTITY, "{bad}: {body}");
        // Validation runs before auth: the same bad body without a token is also 422.
        let (code, _) = put(&f, None, bad.clone()).await;
        assert_eq!(code, StatusCode::UNPROCESSABLE_ENTITY, "{bad}");
    }
    let (_, body) = put(&f, Some(&f.admin), point(-6.25, 9)).await;
    assert_eq!(
        body["message"],
        "Radius absen harus bilangan bulat 10 sampai 1000 meter."
    );
    assert_eq!(get(&f, &f.guard).await["configured"], false);

    let (code, _) = put(&f, None, point(-6.25, 80)).await;
    assert_eq!(code, StatusCode::UNAUTHORIZED);
    let (code, _) = f.app.call("GET", LOCATION, None, None).await;
    assert_eq!(code, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn location_is_scoped_to_the_active_group() {
    let f = fixture().await;
    assert_eq!(
        put(&f, Some(&f.admin), point(-6.25, 80)).await.0,
        StatusCode::OK
    );

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
        assert_eq!(get(&f, token).await["configured"], false);
        let (code, body) = put(&f, Some(token), point(-7.0, 30)).await;
        assert_eq!(code, StatusCode::OK, "{body}");
        assert_eq!(body["data"]["radius"], 30);
    }
    let data = get(&f, &f.admin).await;
    assert_eq!(
        (&data["glat"], &data["radius"]),
        (&json!(-6.25), &json!(80))
    );
}

#[tokio::test]
async fn without_a_group_point_the_shift_location_still_applies() {
    let f = fixture().await;
    let (code, body) = check(&f).await;
    assert_eq!(code, StatusCode::CREATED, "{body}");
    assert_eq!(body["data"]["location_name"], "Pos Utama");
    assert_eq!(body["data"]["radius_meters"], 100);
}

#[tokio::test]
async fn the_group_point_overrides_the_shift_location() {
    let f = fixture().await;

    // The point is about 5.5 km from where the guard stands; the shift's own location would pass.
    assert_eq!(
        put(&f, Some(&f.admin), point(-6.25, 100)).await.0,
        StatusCode::OK
    );
    let (code, body) = check(&f).await;
    assert_eq!(code, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["error_code"], "ATTENDANCE_OUT_OF_RADIUS");
    let (code, body) = f
        .app
        .call(
            "POST",
            "/api/v1/attendance/manual-requests",
            Some(&f.guard),
            Some(&json!({"reason": "camera_unavailable", "latitude": -6.2003, "longitude": 106.8})),
        )
        .await;
    assert_eq!(code, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["error_code"], "ATTENDANCE_OUT_OF_RADIUS");

    let (code, body) = put(&f, Some(&f.admin), point(-6.2005, 50)).await;
    assert_eq!(code, StatusCode::OK, "{body}");
    let (code, body) = check(&f).await;
    assert_eq!(code, StatusCode::CREATED, "{body}");
    assert_eq!(body["data"]["location_name"], "Gerbang Barat");
    assert_eq!(body["data"]["radius_meters"], 50);
}
