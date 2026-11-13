//! `/shifts` against the scratch services. Each test uses its own group and emails.

mod common;

use axum::http::StatusCode;
use chrono::{Datelike, Days, Local};
use common::TestApp;
use kawalo_api::core::security::{JwtUserPayload, sign_access_token};
use serde_json::{Value, json};
use sqlx::MySqlPool;

fn unique() -> String {
    uuid::Uuid::new_v4().simple().to_string()[..12].to_owned()
}

struct Fixture {
    app: TestApp,
    group: String,
    admin: String,
}

async fn token(app: &TestApp, email: &str, group: &str, role: &str, allow_admin: bool) -> String {
    let session_ver = app
        .state
        .sessions
        .get_or_init_version(email)
        .await
        .expect("session version");
    let payload = JwtUserPayload {
        sub: Some(email.to_owned()),
        email: Some(email.to_owned()),
        name: email.split('@').next().map(str::to_owned),
        role: Some(role.to_owned()),
        group_uid: Some(group.to_owned()),
        allow_admin,
        session_ver,
        ..JwtUserPayload::default()
    };
    sign_access_token(&app.state.keys, app.state.config.jwt_expires_in, &payload)
        .expect("sign")
        .token
}

async fn fixture() -> Fixture {
    let app = TestApp::start().await;
    let tag = unique();
    let group = format!("g{tag}");
    let admin = token(&app, &format!("admin.{tag}@test.id"), &group, "ADMIN", true).await;
    Fixture { app, group, admin }
}

fn body(name: &str, start: &str, end: &str) -> Value {
    json!({"name": name, "start_time": start, "end_time": end, "color": "#3b82f6"})
}

async fn call(
    f: &Fixture,
    method: &str,
    path: &str,
    token: Option<&str>,
    b: Option<&Value>,
) -> (StatusCode, Value) {
    f.app.call(method, path, token, b).await
}

async fn create(f: &Fixture, b: &Value) -> Value {
    let (status, res) = call(f, "POST", "/api/v1/shifts", Some(&f.admin), Some(b)).await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    assert_eq!(res["message"], "Shift berhasil dibuat");
    res["data"].clone()
}

async fn list(f: &Fixture, token: &str, query: &str) -> Vec<Value> {
    let (status, res) = call(
        f,
        "GET",
        &format!("/api/v1/shifts{query}"),
        Some(token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{res}");
    res["data"].as_array().expect("array").clone()
}

async fn insert_schedule(
    db: &MySqlPool,
    group: &str,
    shift: &str,
    date: chrono::NaiveDate,
    email: &str,
) {
    sqlx::query(
        "INSERT INTO tkschedule (SchedShiftUID, SDate, SMonth, SYear, SFriendEmail, SFriendName, SGroupUID) \
         VALUES (?, ?, ?, ?, ?, 'Nama', ?)",
    )
    .bind(shift)
    .bind(date.day())
    .bind(date.month())
    .bind(date.year())
    .bind(email)
    .bind(group)
    .execute(db)
    .await
    .expect("insert schedule");
}

#[tokio::test]
async fn authentication_and_validation_order() {
    let f = fixture().await;
    let (status, res) = call(&f, "GET", "/api/v1/shifts", None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{res}");
    // A valid body without a token is 401, a malformed one is 422 (Elysia validates first).
    let (status, _) = call(
        &f,
        "POST",
        "/api/v1/shifts",
        None,
        Some(&body("A", "07:00", "15:00")),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, _) = call(
        &f,
        "POST",
        "/api/v1/shifts",
        None,
        Some(&body("A", "25:99", "15:00")),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let (status, _) = call(
        &f,
        "PUT",
        "/api/v1/shifts/bad%20uid",
        None,
        Some(&body("A", "07:00", "15:00")),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let (status, _) = call(&f, "DELETE", "/api/v1/shifts/bad%20uid", None, None).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn only_admin_roles_write_but_every_member_reads() {
    let f = fixture().await;
    let shift = create(&f, &body("Pagi", "07:00", "15:00")).await;
    let uid = shift["uid"].as_str().expect("uid");
    let tag = unique();
    for (role, allow_admin) in [("GUARD", false), ("USER", false), ("GUARD", true)] {
        let t = token(
            &f.app,
            &format!("{role}.{tag}@test.id"),
            &f.group,
            role,
            allow_admin,
        )
        .await;
        assert_eq!(list(&f, &t, "").await.len(), 1, "{role}");
        let b = body("Lain", "08:00", "16:00");
        let (status, res) = call(&f, "POST", "/api/v1/shifts", Some(&t), Some(&b)).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{role} {res}");
        let (status, _) = call(
            &f,
            "PUT",
            &format!("/api/v1/shifts/{uid}"),
            Some(&t),
            Some(&b),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{role}");
        let (status, _) = call(
            &f,
            "DELETE",
            &format!("/api/v1/shifts/{uid}"),
            Some(&t),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{role}");
    }
    let sa = token(
        &f.app,
        &format!("sa.{tag}@test.id"),
        &f.group,
        "SUPER_ADMIN",
        false,
    )
    .await;
    let (status, res) = call(
        &f,
        "POST",
        "/api/v1/shifts",
        Some(&sa),
        Some(&body("Siang", "12:00", "20:00")),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
}

#[tokio::test]
async fn rejects_invalid_shifts() {
    let f = fixture().await;
    let cases = [
        (body("A", "07:00", "07:00"), StatusCode::BAD_REQUEST),
        (body("A", "7:00", "15:00"), StatusCode::UNPROCESSABLE_ENTITY),
        (
            body("A", "07:00", "24:00"),
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
        (
            body(&"x".repeat(129), "07:00", "15:00"),
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
        (body("   ", "07:00", "15:00"), StatusCode::BAD_REQUEST),
        (
            json!({"name": "A", "start_time": "07:00", "end_time": "15:00", "color": "biru"}),
            StatusCode::BAD_REQUEST,
        ),
    ];
    for (b, expected) in cases {
        let (status, res) = call(&f, "POST", "/api/v1/shifts", Some(&f.admin), Some(&b)).await;
        assert_eq!(status, expected, "{b} -> {res}");
        assert_eq!(res["success"], false);
    }
    let (_, res) = call(
        &f,
        "POST",
        "/api/v1/shifts",
        Some(&f.admin),
        Some(&body("A", "07:00", "07:00")),
    )
    .await;
    assert_eq!(
        res["message"],
        "Jam mulai dan jam selesai tidak boleh sama."
    );
    assert!(list(&f, &f.admin, "").await.is_empty());
}

#[tokio::test]
async fn create_then_list_in_clock_order() {
    let f = fixture().await;
    let mut night = body("Malam", "22:00", "06:00");
    night["description"] = json!("  Jaga malam ");
    // The location moved to the group (`/attendance/location`); old clients' fields are ignored.
    night["location_name"] = json!("Pos Utama");
    night["glat"] = json!(-6.2088);
    night["glong"] = json!(106.8456);
    night["radius"] = json!(100);
    let night = create(&f, &night).await;
    let morning = create(&f, &body("Pagi", "07:00", "15:00")).await;

    assert_eq!(night["uid"].as_str().expect("uid").len(), 32);
    assert_eq!(night["crosses_midnight"], true);
    assert_eq!(night["duration_mins"], 480);
    assert_eq!(night["color"], "#3B82F6");
    assert_eq!(night["description"], "Jaga malam");
    for key in ["location_name", "glat", "glong", "radius"] {
        assert!(night.get(key).is_none(), "{key}");
    }
    assert_eq!(morning["crosses_midnight"], false);
    assert_eq!(morning["description"], Value::Null);

    let names: Vec<_> = list(&f, &f.admin, "")
        .await
        .iter()
        .map(|s| s["name"].clone())
        .collect();
    assert_eq!(names, [json!("Pagi"), json!("Malam")]);
    assert_eq!(list(&f, &f.admin, "?search=mal").await.len(), 1);
}

#[tokio::test]
async fn legacy_rows_are_read_leniently() {
    let f = fixture().await;
    sqlx::query(
        "INSERT INTO tkshift (ShiftName, ShiftTimeStart, ShiftTimeStart_mm, ShiftTimeStop, ShiftTimeStop_mm, \
         ShiftGroupUID, ShiftColor, ShiftUID) VALUES ('Lama', 8, 0, 16, 0, ?, '#112233', ?)",
    )
    .bind(&f.group)
    .bind(format!("SHIFT_{}", unique()))
    .execute(&f.app.state.db)
    .await
    .expect("insert legacy shift");
    let items = list(&f, &f.admin, "").await;
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["description"], Value::Null);
}

#[tokio::test]
async fn groups_are_isolated() {
    let f = fixture().await;
    let shift = create(&f, &body("Pagi", "07:00", "15:00")).await;
    let uid = shift["uid"].as_str().expect("uid");
    let other = token(
        &f.app,
        &format!("a.{}@test.id", unique()),
        &format!("o{}", unique()),
        "ADMIN",
        true,
    )
    .await;
    assert!(list(&f, &other, "").await.is_empty());
    let (status, _) = call(
        &f,
        "PUT",
        &format!("/api/v1/shifts/{uid}"),
        Some(&other),
        Some(&body("X", "07:00", "15:00")),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = call(
        &f,
        "DELETE",
        &format!("/api/v1/shifts/{uid}"),
        Some(&other),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(list(&f, &f.admin, "").await.len(), 1);
}

#[tokio::test]
async fn names_are_unique_per_group_ignoring_case() {
    let f = fixture().await;
    let pagi = create(&f, &body("Pagi", "07:00", "15:00")).await;
    let siang = create(&f, &body("Siang", "12:00", "20:00")).await;
    let (status, res) = call(
        &f,
        "POST",
        "/api/v1/shifts",
        Some(&f.admin),
        Some(&body("pAGI ", "08:00", "16:00")),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{res}");
    assert_eq!(res["error_code"], "CONFLICT_ERROR");
    assert_eq!(res["message"], "Nama shift sudah dipakai di grup ini.");

    // Saving a shift under its own name is fine; taking another one's is not.
    let siang_uid = siang["uid"].as_str().expect("uid");
    let (status, _) = call(
        &f,
        "PUT",
        &format!("/api/v1/shifts/{siang_uid}"),
        Some(&f.admin),
        Some(&body("Siang", "13:00", "21:00")),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = call(
        &f,
        "PUT",
        &format!("/api/v1/shifts/{siang_uid}"),
        Some(&f.admin),
        Some(&body("PAGI", "13:00", "21:00")),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);

    // A deleted shift gives its name back.
    let pagi_uid = pagi["uid"].as_str().expect("uid");
    let (status, _) = call(
        &f,
        "DELETE",
        &format!("/api/v1/shifts/{pagi_uid}"),
        Some(&f.admin),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    create(&f, &body("Pagi", "07:00", "15:00")).await;
}

#[tokio::test]
async fn put_replaces_the_whole_shift() {
    let f = fixture().await;
    let mut full = body("Pagi", "07:00", "15:00");
    full["description"] = json!("Catatan");
    let shift = create(&f, &full).await;
    let uid = shift["uid"].as_str().expect("uid");
    let (status, res) = call(
        &f,
        "PUT",
        &format!("/api/v1/shifts/{uid}"),
        Some(&f.admin),
        Some(&body("Pagi 2", "06:30", "14:30")),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{res}");
    assert_eq!(res["message"], "Shift berhasil diperbarui");
    assert_eq!(res["data"]["name"], "Pagi 2");
    assert_eq!(res["data"]["start_time"], "06:30");
    assert_eq!(res["data"]["description"], Value::Null);
    let (status, _) = call(
        &f,
        "PUT",
        "/api/v1/shifts/tidakada",
        Some(&f.admin),
        Some(&body("Z", "06:30", "14:30")),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn delete_is_blocked_by_upcoming_schedules_and_otherwise_soft() {
    let f = fixture().await;
    let db = &f.app.state.db;
    let busy = create(&f, &body("Sibuk", "07:00", "15:00")).await;
    let idle = create(&f, &body("Senggang", "08:00", "16:00")).await;
    let busy_uid = busy["uid"].as_str().expect("uid");
    let idle_uid = idle["uid"].as_str().expect("uid");
    let today = Local::now().date_naive();
    let yesterday = today.checked_sub_days(Days::new(1)).expect("date");
    let email = format!("budi.{}@test.id", unique());
    insert_schedule(db, &f.group, busy_uid, today, &email).await;
    insert_schedule(
        db,
        &f.group,
        busy_uid,
        today.checked_add_days(Days::new(3)).expect("date"),
        &email,
    )
    .await;
    insert_schedule(db, &f.group, idle_uid, yesterday, &email).await;

    let (status, res) = call(
        &f,
        "DELETE",
        &format!("/api/v1/shifts/{busy_uid}"),
        Some(&f.admin),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{res}");
    assert_eq!(res["error_code"], "CONFLICT_ERROR");
    assert!(
        res["message"]
            .as_str()
            .expect("message")
            .contains("2 jadwal"),
        "{res}"
    );
    assert_eq!(list(&f, &f.admin, "").await.len(), 2);

    let (status, res) = call(
        &f,
        "DELETE",
        &format!("/api/v1/shifts/{idle_uid}"),
        Some(&f.admin),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{res}");
    assert_eq!(res["message"], "Shift berhasil dihapus");
    assert_eq!(list(&f, &f.admin, "").await.len(), 1);
    let (status, _) = call(
        &f,
        "PUT",
        &format!("/api/v1/shifts/{idle_uid}"),
        Some(&f.admin),
        Some(&body("Senggang", "08:00", "16:00")),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = call(
        &f,
        "DELETE",
        &format!("/api/v1/shifts/{idle_uid}"),
        Some(&f.admin),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // History survives: the row is still there and the past schedule still shows its shift.
    let remaining: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM tkshift WHERE ShiftUID = ? AND ShiftDeletedAt IS NOT NULL",
    )
    .bind(idle_uid)
    .fetch_one(db)
    .await
    .expect("count");
    assert_eq!(remaining, 1);
    let member = token(&f.app, &email, &f.group, "GUARD", false).await;
    let day = yesterday.format("%Y-%m-%d");
    let (status, res) = call(
        &f,
        "GET",
        &format!("/api/v1/schedules/me?from={day}&to={day}"),
        Some(&member),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{res}");
    assert_eq!(res["data"].as_array().expect("array").len(), 1);
    assert_eq!(res["data"][0]["shift_name"], "Senggang");
}
