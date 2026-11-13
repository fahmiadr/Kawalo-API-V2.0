//! Schedule management (`/schedules` CRUD and bulk) against the scratch services. Each test uses
//! its own group and emails.

mod common;

use axum::http::StatusCode;
use chrono::{Datelike, Days, Local, NaiveDate};
use common::TestApp;
use serde_json::{Value, json};
use sqlx::MySqlPool;

fn unique() -> String {
    uuid::Uuid::new_v4().simple().to_string()[..12].to_owned()
}

fn day(offset: i64) -> NaiveDate {
    let today = Local::now().date_naive();
    if offset >= 0 {
        today.checked_add_days(Days::new(offset.unsigned_abs()))
    } else {
        today.checked_sub_days(Days::new(offset.unsigned_abs()))
    }
    .expect("date")
}

fn ymd(offset: i64) -> String {
    day(offset).format("%Y-%m-%d").to_string()
}

struct Fixture {
    app: TestApp,
    group: String,
    admin: String,
    guard: String,
    members: Vec<String>,
    pagi: String,
    malam: String,
}

async fn insert_member(db: &MySqlPool, group: &str, email: &str, status: &str) {
    sqlx::query(
        "INSERT INTO tkusergroup (Email, GroupUID, Category, Status) VALUES (?, ?, 'GUARD', ?)",
    )
    .bind(email)
    .bind(group)
    .bind(status)
    .execute(db)
    .await
    .expect("insert member");
}

async fn insert_shift(db: &MySqlPool, group: &str, uid: &str, start: i32, deleted: bool) {
    sqlx::query(
        "INSERT INTO tkshift (ShiftName, ShiftTimeStart, ShiftTimeStart_mm, ShiftTimeStop, \
         ShiftTimeStop_mm, ShiftGroupUID, ShiftColor, ShiftUID, ShiftDeletedAt) \
         VALUES (?, ?, 0, ?, 0, ?, '#112233', ?, IF(?, NOW(), NULL))",
    )
    .bind(format!("Shift {uid}"))
    .bind(start)
    .bind(start + 8)
    .bind(group)
    .bind(uid)
    .bind(deleted)
    .execute(db)
    .await
    .expect("insert shift");
}

async fn fixture() -> Fixture {
    let app = TestApp::start().await;
    let tag = unique();
    let group = format!("g{tag}");
    let admin = format!("admin.{tag}@test.id");
    let guard = format!("guard.{tag}@test.id");
    let members: Vec<String> = (0..3).map(|i| format!("m{i}.{tag}@test.id")).collect();
    let db = &app.state.db;
    for email in &members {
        insert_member(db, &group, email, "JOINED").await;
    }
    let (pagi, malam) = (format!("pg{tag}"), format!("ml{tag}"));
    insert_shift(db, &group, &pagi, 7, false).await;
    insert_shift(db, &group, &malam, 23, false).await;
    let admin_token = app.token_for(&admin, &group, true).await;
    let guard_token = app.token_for(&guard, &group, false).await;
    Fixture {
        app,
        group,
        admin: admin_token,
        guard: guard_token,
        members,
        pagi,
        malam,
    }
}

impl Fixture {
    async fn call(&self, method: &str, path: &str, body: Option<&Value>) -> (StatusCode, Value) {
        self.app
            .call(method, &format!("/api/v1{path}"), Some(&self.admin), body)
            .await
    }

    async fn create(
        &self,
        member: usize,
        shift: &str,
        offset: i64,
        extra: Value,
    ) -> (StatusCode, Value) {
        let mut body =
            json!({ "email": self.members[member], "shift_uid": shift, "date": ymd(offset) });
        body.as_object_mut()
            .expect("object")
            .extend(extra.as_object().expect("object").clone());
        self.call("POST", "/schedules", Some(&body)).await
    }

    async fn checkin(&self, id: &str) {
        sqlx::query("UPDATE tkschedule SET ACheckin = '08:00' WHERE SchedId = ?")
            .bind(id.parse::<i32>().expect("id"))
            .execute(&self.app.state.db)
            .await
            .expect("check in");
    }

    async fn insert_raw(&self, member: usize, shift: &str, date: NaiveDate) -> String {
        sqlx::query(
            "INSERT INTO tkschedule (SchedShiftUID, SDate, SMonth, SYear, SFriendEmail, \
             SFriendName, SGroupUID) VALUES (?, ?, ?, ?, ?, 'Nama', ?)",
        )
        .bind(shift)
        .bind(i32::try_from(date.day()).expect("day"))
        .bind(i32::try_from(date.month()).expect("month"))
        .bind(date.year())
        .bind(&self.members[member])
        .bind(&self.group)
        .execute(&self.app.state.db)
        .await
        .expect("insert schedule")
        .last_insert_id()
        .to_string()
    }
}

#[tokio::test]
async fn only_admins_manage_the_group_schedule() {
    let f = fixture().await;
    let body = json!({ "email": f.members[0], "shift_uid": f.pagi, "date": ymd(1) });
    for (method, path, payload) in [
        ("GET", "/api/v1/schedules", None),
        ("POST", "/api/v1/schedules", Some(&body)),
        ("DELETE", "/api/v1/schedules/1", None),
    ] {
        let (status, json) = f.app.call(method, path, Some(&f.guard), payload).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{method} {path}: {json}");
        let (status, _) = f.app.call(method, path, None, payload).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{method} {path}");
    }
}

#[tokio::test]
async fn create_validates_and_enforces_one_shift_per_day() {
    let f = fixture().await;
    let (status, body) = f.create(0, &f.pagi, 2, json!({})).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["message"], "Jadwal berhasil dibuat");
    let item = &body["data"];
    assert_eq!(item["date"], ymd(2));
    assert_eq!(item["shift_uid"], f.pagi);
    assert_eq!(item["start_time"], "07:00");
    assert_eq!(item["is_member"], true);
    assert_eq!(item["locked"], false);
    assert_eq!(item["avatar_url"], Value::Null, "no tkuser row");

    let (status, body) = f.create(0, &f.pagi, 2, json!({})).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error_code"], "CONFLICT_ERROR");
    assert_eq!(
        body["message"],
        "Jadwal personil ini pada tanggal dan shift tersebut sudah ada."
    );
    let (status, body) = f.create(0, &f.malam, 2, json!({})).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error_code"], "SCHEDULE_OTHER_SHIFT");
    assert_eq!(
        body["message"],
        "Personil sudah punya jadwal shift lain pada tanggal ini."
    );

    let db = &f.app.state.db;
    let tag = unique();
    let gone = format!("gone{tag}");
    insert_shift(db, &f.group, &gone, 1, true).await;
    insert_member(db, &f.group, &format!("left.{tag}@test.id"), "LEFT").await;
    for (member, shift, offset, message) in [
        (
            1,
            f.pagi.as_str(),
            -1,
            "Jadwal untuk tanggal yang sudah lewat tidak bisa dibuat atau diubah.",
        ),
        (
            1,
            f.pagi.as_str(),
            400,
            "Jadwal maksimal 366 hari ke depan.",
        ),
        (1, gone.as_str(), 3, "Shift tidak ditemukan di grup ini."),
        (1, "tidak-ada", 3, "Shift tidak ditemukan di grup ini."),
    ] {
        let (status, body) = f.create(member, shift, offset, json!({})).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{message}: {body}");
        assert_eq!(body["message"], message);
    }
    let outsider =
        json!({ "email": format!("left.{tag}@test.id"), "shift_uid": f.pagi, "date": ymd(3) });
    let (status, body) = f.call("POST", "/schedules", Some(&outsider)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["message"], "Personil bukan anggota aktif grup ini.");

    let bad_date = json!({ "email": f.members[1], "shift_uid": f.pagi, "date": "2026-6-3" });
    let (status, _) = f.call("POST", "/schedules", Some(&bad_date)).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn overwrite_needs_explicit_confirmation_and_spares_locked_rows() {
    let f = fixture().await;
    let (_, body) = f.create(0, &f.pagi, 2, json!({})).await;
    let id = body["data"]["id"].as_str().expect("id").to_owned();

    let (status, _) = f
        .create(0, &f.malam, 2, json!({ "overwrite": false }))
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
    let (status, body) = f.create(0, &f.malam, 2, json!({ "overwrite": true })).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["data"]["shift_uid"], f.malam);
    let (_, list) = f
        .call("GET", &format!("/schedules?from={0}&to={0}", ymd(2)), None)
        .await;
    let rows = list["data"].as_array().expect("rows");
    assert_eq!(rows.len(), 1);
    assert_ne!(rows[0]["id"], id.as_str());

    f.checkin(rows[0]["id"].as_str().expect("id")).await;
    let (status, body) = f.create(0, &f.pagi, 2, json!({ "overwrite": true })).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(
        body["message"],
        "Jadwal yang sudah lewat atau sudah diabsen tidak bisa diubah."
    );
}

#[tokio::test]
async fn update_moves_rows_and_locks_past_or_attended_ones() {
    let f = fixture().await;
    let (_, body) = f.create(0, &f.pagi, 2, json!({})).await;
    let id = body["data"]["id"].as_str().expect("id").to_owned();
    f.create(1, &f.pagi, 3, json!({})).await;

    let path = format!("/schedules/{id}");
    let target = json!({ "email": f.members[2], "shift_uid": f.malam, "date": ymd(4) });
    let (status, body) = f.call("PUT", &path, Some(&target)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["message"], "Jadwal berhasil diperbarui");
    assert_eq!(body["data"]["email"], f.members[2]);
    assert_eq!(body["data"]["date"], ymd(4));

    let (status, _) = f.call("PUT", &path, Some(&target)).await;
    assert_eq!(status, StatusCode::OK, "unchanged is a no-op");

    let clash = json!({ "email": f.members[1], "shift_uid": f.malam, "date": ymd(3) });
    let (status, _) = f.call("PUT", &path, Some(&clash)).await;
    assert_eq!(status, StatusCode::CONFLICT);

    f.checkin(&id).await;
    let (status, body) = f.call("PUT", &path, Some(&target)).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");

    let past = f.insert_raw(0, &f.pagi, day(-2)).await;
    let (status, _) = f
        .call("PUT", &format!("/schedules/{past}"), Some(&target))
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
    let (status, _) = f.call("PUT", "/schedules/2147483000", Some(&target)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn delete_single_and_batch_skip_locked_rows() {
    let f = fixture().await;
    let mut ids = Vec::new();
    for (member, offset) in [(0, 2), (1, 2), (2, 2)] {
        let (_, body) = f.create(member, &f.pagi, offset, json!({})).await;
        ids.push(body["data"]["id"].as_str().expect("id").to_owned());
    }
    let past = f.insert_raw(0, &f.pagi, day(-1)).await;

    let (status, _) = f.call("DELETE", &format!("/schedules/{past}"), None).await;
    assert_eq!(status, StatusCode::CONFLICT);
    let (status, body) = f
        .call("DELETE", &format!("/schedules/{}", ids[0]), None)
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["message"], "Jadwal berhasil dihapus");
    let (status, _) = f
        .call("DELETE", &format!("/schedules/{}", ids[0]), None)
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    f.checkin(&ids[1]).await;
    let batch = json!({ "ids": [ids[1], ids[2], past, "999999999"] });
    let (status, body) = f.call("DELETE", "/schedules", Some(&batch)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body["data"],
        json!({ "deleted_count": 1, "skipped_count": 3 })
    );

    let none = json!({ "ids": ["999999998"] });
    let (status, _) = f.call("DELETE", "/schedules", Some(&none)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn bulk_reports_skips_rejects_invalid_items_and_is_idempotent() {
    let f = fixture().await;
    let item = |m: usize, s: &str, o: i64| json!({ "email": f.members[m], "shift_uid": s, "date": ymd(o) });
    let body = json!({ "items": [
        item(0, &f.pagi, 2),
        item(0, &f.pagi, 2),
        item(0, &f.malam, 2),
        item(1, &f.malam, 2),
        item(2, &f.pagi, 400 - 40),
    ]});
    let (status, res) = f.call("POST", "/schedules/bulk", Some(&body)).await;
    assert_eq!(status, StatusCode::CREATED, "{res}");
    assert_eq!(res["data"]["created"], 3);
    assert_eq!(res["data"]["skipped"], 2);
    assert_eq!(res["data"]["skipped_items"][0]["index"], 1);
    assert_eq!(res["data"]["skipped_items"][0]["reason"], "Sudah ada");
    assert_eq!(res["data"]["skipped_items"][1]["index"], 2);
    assert!(res["data"]["skipped_items"][0].get("code").is_none());
    assert_eq!(
        res["data"]["skipped_items"][1]["code"],
        "SCHEDULE_OTHER_SHIFT"
    );

    let (status, res) = f.call("POST", "/schedules/bulk", Some(&body)).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(res["data"]["created"], 0);

    let invalid = json!({ "items": [item(0, &f.pagi, 5), item(1, &f.pagi, -1)] });
    let (status, res) = f.call("POST", "/schedules/bulk", Some(&invalid)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{res}");
    assert!(
        res["message"]
            .as_str()
            .expect("message")
            .starts_with("Item 1:")
    );
    let (_, list) = f
        .call("GET", &format!("/schedules?from={0}&to={0}", ymd(5)), None)
        .await;
    assert!(
        list["data"].as_array().expect("rows").is_empty(),
        "all or nothing"
    );

    let swap = json!({ "overwrite": true, "items": [item(0, &f.malam, 2)] });
    let (_, res) = f.call("POST", "/schedules/bulk", Some(&swap)).await;
    assert_eq!(res["data"]["created"], 1);
    assert_eq!(res["data"]["replaced"], 1);

    for bad in [
        json!({ "items": [] }),
        json!({}),
        json!({ "items": [{ "email": 1 }] }),
    ] {
        let (status, _) = f.call("POST", "/schedules/bulk", Some(&bad)).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{bad}");
    }
    let too_many = json!({ "items": vec![item(0, &f.pagi, 6); 1001] });
    let (status, _) = f.call("POST", "/schedules/bulk", Some(&too_many)).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn list_is_scoped_to_the_group_and_filters_by_person_and_shift() {
    let f = fixture().await;
    f.create(0, &f.pagi, 2, json!({})).await;
    f.create(1, &f.malam, 2, json!({})).await;
    for (member, pic) in [(0, "https://cdn.test/a.webp"), (1, "N/A")] {
        sqlx::query(
            "INSERT INTO tkuser (UserUID, UserEmail, UserName, Password, UserProfilePic) \
             VALUES (?, ?, 'Nama', 'x', ?)",
        )
        .bind(unique())
        .bind(&f.members[member])
        .bind(pic)
        .execute(&f.app.state.db)
        .await
        .expect("insert user");
    }

    let other = TestApp::start().await;
    let other_group = format!("o{}", unique());
    let admin = other.token_for("x@test.id", &other_group, true).await;
    let (_, body) = other
        .call("GET", "/api/v1/schedules", Some(&admin), None)
        .await;
    assert!(body["data"].as_array().expect("rows").is_empty());

    let range = format!("from={0}&to={0}", ymd(2));
    let (status, body) = f.call("GET", &format!("/schedules?{range}"), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["message"], "Jadwal berhasil diambil");
    assert_eq!(body["data"].as_array().expect("rows").len(), 2);
    let avatars: Vec<(&Value, &Value)> = body["data"]
        .as_array()
        .expect("rows")
        .iter()
        .map(|row| (&row["email"], &row["avatar_url"]))
        .collect();
    assert!(avatars.contains(&(&json!(f.members[0]), &json!("https://cdn.test/a.webp"))));
    assert!(
        avatars.contains(&(&json!(f.members[1]), &Value::Null)),
        "N/A is cleaned"
    );
    let (_, body) = f
        .call(
            "GET",
            &format!("/schedules?{range}&email={}", f.members[1]),
            None,
        )
        .await;
    assert_eq!(body["data"].as_array().expect("rows").len(), 1);
    let (_, body) = f
        .call(
            "GET",
            &format!("/schedules?{range}&shift_uid={}", f.pagi),
            None,
        )
        .await;
    assert_eq!(body["data"][0]["email"], f.members[0]);

    let (status, _) = f
        .call("GET", "/schedules?from=2026-01-01&to=2026-12-31", None)
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn list_shows_the_group_attendance_point_as_location() {
    let f = fixture().await;
    f.create(0, &f.pagi, 2, json!({})).await;
    let range = format!("/schedules?from={0}&to={0}", ymd(2));
    let (_, body) = f.call("GET", &range, None).await;
    assert_eq!(body["data"][0]["location_name"], Value::Null);

    sqlx::query(
        "INSERT INTO tkattendancelocation (GroupUID, LocName, LocLat, LocLong, LocRadius, \
         UpdatedBy) VALUES (?, 'Gerbang Barat', -6.2, 106.8, 50, 'admin@test.id')",
    )
    .bind(&f.group)
    .execute(&f.app.state.db)
    .await
    .expect("group point");
    let (_, body) = f.call("GET", &range, None).await;
    assert_eq!(body["data"][0]["location_name"], "Gerbang Barat");
}
