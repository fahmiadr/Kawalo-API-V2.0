//! Schedule forecast (`POST /schedules/generate/preview` and `/schedules/generate`) against the
//! scratch services. Each test uses its own group and emails.

mod common;

use axum::http::StatusCode;
use chrono::{Days, Local, NaiveDate};
use common::TestApp;
use kawalo_api::core::security::{JwtUserPayload, sign_access_token};
use serde_json::{Value, json};
use sqlx::MySqlPool;

const PREVIEW: &str = "/api/v1/schedules/generate/preview";
const COMMIT: &str = "/api/v1/schedules/generate";

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

async fn insert_member(db: &MySqlPool, group: &str, email: &str, name: &str, category: &str) {
    sqlx::query(
        "INSERT INTO tkuser (UserUID, UserEmail, UserName, Password) VALUES (?, ?, ?, 'x')",
    )
    .bind(format!("u-{}", unique()))
    .bind(email)
    .bind(name)
    .execute(db)
    .await
    .expect("insert user");
    sqlx::query(
        "INSERT INTO tkusergroup (Email, GroupUID, Category, Status) VALUES (?, ?, ?, 'JOINED')",
    )
    .bind(email)
    .bind(group)
    .bind(category)
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
    .bind((start + 8) % 24)
    .bind(group)
    .bind(uid)
    .bind(deleted)
    .execute(db)
    .await
    .expect("insert shift");
}

async fn token_as(app: &TestApp, email: &str, group: &str, role: &str) -> String {
    let session_ver = app
        .state
        .sessions
        .get_or_init_version(email)
        .await
        .expect("session version");
    let payload = JwtUserPayload {
        sub: Some(email.to_owned()),
        email: Some(email.to_owned()),
        role: Some(role.to_owned()),
        group_uid: Some(group.to_owned()),
        session_ver,
        ..JwtUserPayload::default()
    };
    sign_access_token(&app.state.keys, app.state.config.jwt_expires_in, &payload)
        .expect("sign")
        .token
}

struct Fixture {
    app: TestApp,
    group: String,
    admin: String,
    guard: String,
    /// Two GUARDs then one USER, all JOINED.
    members: Vec<String>,
    pagi: String,
    malam: String,
}

async fn fixture() -> Fixture {
    let app = TestApp::start().await;
    let tag = unique();
    let group = format!("g{tag}");
    let db = &app.state.db;
    let members: Vec<String> = ["budi", "ani", "cici"]
        .iter()
        .map(|n| format!("{n}.{tag}@test.id"))
        .collect();
    insert_member(db, &group, &members[0], "Budi", "GUARD").await;
    insert_member(db, &group, &members[1], "Ani", "GUARD").await;
    insert_member(db, &group, &members[2], "Cici", "USER").await;
    let (pagi, malam) = (format!("p{tag}"), format!("m{tag}"));
    insert_shift(db, &group, &pagi, 7, false).await;
    insert_shift(db, &group, &malam, 22, false).await;
    let admin = app
        .token_for(&format!("admin.{tag}@test.id"), &group, true)
        .await;
    let guard = app.token_for(&members[0], &group, false).await;
    Fixture {
        app,
        group,
        admin,
        guard,
        members,
        pagi,
        malam,
    }
}

impl Fixture {
    fn body(&self, from: i64, to: i64) -> Value {
        json!({
            "start_date": ymd(from),
            "end_date": ymd(to),
            "shifts": [{"shift_uid": self.pagi, "quota": 1}, {"shift_uid": self.malam, "quota": 1}],
            "personnel": self.members,
            "seed": 12345,
        })
    }

    async fn call(&self, path: &str, token: Option<&str>, body: &Value) -> (StatusCode, Value) {
        self.app.call("POST", path, token, Some(body)).await
    }

    async fn preview(&self, body: &Value) -> Value {
        let (status, json) = self.call(PREVIEW, Some(&self.admin), body).await;
        assert_eq!(status, StatusCode::OK, "{json}");
        assert_eq!(json["message"], "Pratinjau jadwal otomatis dibuat");
        json["data"].clone()
    }

    async fn commit(&self, body: &Value, preview: &Value) -> (StatusCode, Value) {
        let mut body = body.clone();
        body["seed"] = preview["seed"].clone();
        body["preview_token"] = preview["preview_token"].clone();
        self.call(COMMIT, Some(&self.admin), &body).await
    }

    async fn count(&self) -> i64 {
        sqlx::query_scalar("SELECT COUNT(*) FROM tkschedule WHERE SGroupUID = ?")
            .bind(&self.group)
            .fetch_one(&self.app.state.db)
            .await
            .expect("count")
    }

    async fn insert_row(&self, email: &str, shift: &str, date: NaiveDate, checkin: &str) -> u64 {
        use chrono::Datelike;
        sqlx::query(
            "INSERT INTO tkschedule (SchedShiftUID, SDate, SMonth, SYear, SFriendEmail, \
             SFriendName, SGroupUID, LateMins, ACheckin) VALUES (?, ?, ?, ?, ?, 'Nama', ?, 0, ?)",
        )
        .bind(shift)
        .bind(date.day())
        .bind(date.month())
        .bind(date.year())
        .bind(email)
        .bind(&self.group)
        .bind(checkin)
        .execute(&self.app.state.db)
        .await
        .expect("insert schedule")
        .last_insert_id()
    }

    /// `date|shift|email|name|SchedDT`, ordered.
    async fn rows(&self) -> Vec<String> {
        let rows: Vec<(i32, i32, i32, String, String, String, String)> = sqlx::query_as(
            "SELECT SYear, SMonth, SDate, SchedShiftUID, SFriendEmail, SFriendName, \
             DATE_FORMAT(SchedDT, '%Y-%m-%d %H:%i:%s') FROM tkschedule WHERE SGroupUID = ? \
             ORDER BY SYear, SMonth, SDate, SchedShiftUID, SFriendEmail",
        )
        .bind(&self.group)
        .fetch_all(&self.app.state.db)
        .await
        .expect("rows");
        rows.into_iter()
            .map(|(y, m, d, s, e, n, dt)| format!("{y:04}-{m:02}-{d:02}|{s}|{e}|{n}|{dt}"))
            .collect()
    }
}

fn plan_rows(preview: &Value) -> Vec<String> {
    let mut rows: Vec<String> = preview["plan"]
        .as_array()
        .expect("plan")
        .iter()
        .map(|p| {
            format!(
                "{}|{}|{}|{}|{} 01:00:00",
                p["date"].as_str().expect("date"),
                p["shift_uid"].as_str().expect("shift"),
                p["email"].as_str().expect("email"),
                p["name"].as_str().expect("name"),
                p["date"].as_str().expect("date"),
            )
        })
        .collect();
    rows.sort();
    rows
}

#[tokio::test]
async fn only_admins_of_the_group_may_generate() {
    let f = fixture().await;
    let body = f.body(0, 2);
    let (status, json) = f.call(PREVIEW, None, &body).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{json}");
    let (status, json) = f.call(PREVIEW, Some(&f.guard), &body).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{json}");
    let super_admin = token_as(
        &f.app,
        &format!("su.{}@test.id", unique()),
        &f.group,
        "SUPER_ADMIN",
    )
    .await;
    let (status, json) = f.call(PREVIEW, Some(&super_admin), &body).await;
    assert_eq!(status, StatusCode::OK, "{json}");

    let mut commit = body.clone();
    commit["preview_token"] = json!("00");
    let (status, _) = f.call(COMMIT, Some(&f.guard), &commit).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn preview_is_deterministic_and_writes_nothing() {
    let f = fixture().await;
    let body = f.body(0, 6);
    let first = f.preview(&body).await;
    let second = f.preview(&body).await;
    assert_eq!(first, second);
    assert_eq!(first["seed"], 12345);
    assert_eq!(first["fill_mode"], "empty");
    assert_eq!(first["stats"]["total_created"], 14);
    assert_eq!(first["stats"]["total_capacity"], 14);
    assert_eq!(f.count().await, 0);

    // Without a seed the server draws one and returns it; replaying it gives the same plan.
    let mut random = body.clone();
    random.as_object_mut().expect("object").remove("seed");
    let drawn = f.preview(&random).await;
    let seed = drawn["seed"].as_u64().expect("seed");
    assert!(seed <= 2_147_483_647);
    random["seed"] = json!(seed);
    assert_eq!(f.preview(&random).await["plan"], drawn["plan"]);
}

#[tokio::test]
async fn omitted_personnel_defaults_to_the_guards() {
    let f = fixture().await;
    let mut body = f.body(0, 3);
    body.as_object_mut().expect("object").remove("personnel");
    let preview = f.preview(&body).await;
    let mut emails: Vec<&str> = preview["stats"]["per_personnel"]
        .as_array()
        .expect("per_personnel")
        .iter()
        .map(|p| p["email"].as_str().expect("email"))
        .collect();
    emails.sort_unstable();
    let mut guards = vec![f.members[0].as_str(), f.members[1].as_str()];
    guards.sort_unstable();
    assert_eq!(emails, guards);
}

#[tokio::test]
async fn per_personnel_carries_the_cleaned_avatar() {
    let f = fixture().await;
    for (member, pic) in [(0, "https://cdn.test/m0.webp"), (1, "null")] {
        sqlx::query("UPDATE tkuser SET UserProfilePic = ? WHERE UserEmail = ?")
            .bind(pic)
            .bind(&f.members[member])
            .execute(&f.app.state.db)
            .await
            .expect("set picture");
    }
    let preview = f.preview(&f.body(0, 3)).await;
    let people = preview["stats"]["per_personnel"]
        .as_array()
        .expect("per_personnel");
    let avatar = |email: &str| {
        people
            .iter()
            .find(|p| p["email"] == email)
            .map(|p| p["avatar_url"].clone())
    };
    assert_eq!(
        avatar(&f.members[0]),
        Some(json!("https://cdn.test/m0.webp"))
    );
    assert_eq!(avatar(&f.members[1]), Some(Value::Null));
}

#[tokio::test]
async fn rejects_bad_ranges_shifts_and_people() {
    let f = fixture().await;
    let db = &f.app.state.db;
    let tag = unique();
    let (foreign, gone) = (format!("f{tag}"), format!("d{tag}"));
    insert_shift(db, &format!("other{tag}"), &foreign, 8, false).await;
    insert_shift(db, &f.group, &gone, 8, true).await;

    let mut cases: Vec<(Value, StatusCode, &str)> = vec![
        (
            f.body(0, 92),
            StatusCode::BAD_REQUEST,
            "Rentang tanggal maksimal 92 hari.",
        ),
        (
            f.body(-1, 3),
            StatusCode::BAD_REQUEST,
            "Tanggal mulai tidak boleh sebelum hari ini.",
        ),
        (
            f.body(3, 1),
            StatusCode::BAD_REQUEST,
            "Tanggal akhir tidak boleh sebelum tanggal mulai.",
        ),
    ];
    for uid in [&foreign, &gone] {
        let mut b = f.body(0, 1);
        b["shifts"] = json!([{"shift_uid": uid, "quota": 1}]);
        cases.push((
            b,
            StatusCode::BAD_REQUEST,
            "Shift tidak ditemukan di grup ini",
        ));
    }
    let mut b = f.body(0, 1);
    b["personnel"] = json!([format!("asing.{tag}@test.id")]);
    cases.push((
        b,
        StatusCode::BAD_REQUEST,
        "Personil bukan anggota aktif grup ini",
    ));
    let mut b = f.body(0, 1);
    b["start_date"] = json!("2026-1-1");
    cases.push((
        b,
        StatusCode::UNPROCESSABLE_ENTITY,
        "Format data masukan tidak valid",
    ));
    let mut b = f.body(0, 1);
    b["shifts"] = json!([{"shift_uid": f.pagi, "quota": 51}]);
    cases.push((
        b,
        StatusCode::UNPROCESSABLE_ENTITY,
        "Format data masukan tidak valid",
    ));

    for (body, status, message) in cases {
        let (got, json) = f.call(PREVIEW, Some(&f.admin), &body).await;
        assert_eq!(got, status, "{body}: {json}");
        assert!(
            json["message"]
                .as_str()
                .expect("message")
                .starts_with(message),
            "{json}"
        );
    }
    assert_eq!(f.count().await, 0);
}

#[tokio::test]
async fn commit_writes_exactly_the_previewed_plan() {
    let f = fixture().await;
    let body = f.body(0, 6);
    let preview = f.preview(&body).await;
    let (status, json) = f.commit(&body, &preview).await;
    assert_eq!(status, StatusCode::CREATED, "{json}");
    assert_eq!(json["data"]["created"], 14);
    assert_eq!(json["data"]["deleted"], 0);
    assert_eq!(
        json["message"],
        "Jadwal otomatis tersimpan. Dibuat: 14, Dihapus: 0."
    );
    assert_eq!(f.rows().await, plan_rows(&preview));

    // The slots are full now: the same request plans nothing and the commit says so.
    let again = f.preview(&body).await;
    assert_eq!(again["stats"]["total_created"], 0);
    let (status, json) = f.commit(&body, &again).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{json}");
    assert_eq!(json["message"], "Tidak ada jadwal untuk disimpan.");
}

#[tokio::test]
async fn stale_or_forged_tokens_are_409_and_write_nothing() {
    let f = fixture().await;
    let body = f.body(0, 3);
    let preview = f.preview(&body).await;

    let mut forged = preview.clone();
    forged["preview_token"] = json!("0".repeat(64));
    let (status, json) = f.commit(&body, &forged).await;
    assert_eq!(status, StatusCode::CONFLICT, "{json}");
    assert_eq!(
        json["message"],
        "Data jadwal berubah sejak pratinjau dibuat. Buat pratinjau ulang."
    );

    // Another seed is another plan: the token no longer matches.
    let mut reseeded = preview.clone();
    reseeded["seed"] = json!(1);
    let (status, _) = f.commit(&body, &reseeded).await;
    assert_eq!(status, StatusCode::CONFLICT);

    // A row added after the preview changes the plan.
    f.insert_row(&f.members[2], &f.pagi, day(1), "N/A").await;
    let (status, _) = f.commit(&body, &preview).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(f.count().await, 1);
}

#[tokio::test]
async fn overwrite_needs_confirmation_and_keeps_locked_rows() {
    let f = fixture().await;
    let attended = f
        .insert_row(&f.members[0], &f.pagi, day(0), "2026-01-01 07:01")
        .await;
    let open = f.insert_row(&f.members[1], &f.malam, day(1), "N/A").await;
    let outside = f.insert_row(&f.members[1], &f.malam, day(10), "N/A").await;

    let mut body = f.body(0, 2);
    body["fill_mode"] = json!("overwrite");
    let preview = f.preview(&body).await;
    assert_eq!(preview["existing_count"], 2);
    assert_eq!(preview["delete_count"], 1);
    assert_eq!(preview["locked_count"], 1);

    let (status, json) = f.commit(&body, &preview).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{json}");
    assert_eq!(f.count().await, 3);

    body["confirm_overwrite"] = json!(true);
    let (status, json) = f.commit(&body, &preview).await;
    assert_eq!(status, StatusCode::CREATED, "{json}");
    assert_eq!(json["data"]["deleted"], 1);
    let mut ids: Vec<i64> = sqlx::query_scalar(
        "SELECT SchedId FROM tkschedule WHERE SGroupUID = ? AND SchedId IN (?, ?, ?)",
    )
    .bind(&f.group)
    .bind(attended)
    .bind(open)
    .bind(outside)
    .fetch_all(&f.app.state.db)
    .await
    .expect("ids");
    ids.sort_unstable();
    let expected: Vec<i64> = [attended, outside]
        .iter()
        .map(|id| i64::try_from(*id).expect("id"))
        .collect();
    assert_eq!(ids, expected);
    // The locked row still fills its own slot: one person on today's Pagi, as the quota says.
    let today_pagi: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM tkschedule WHERE SGroupUID = ? AND SchedShiftUID = ? \
         AND SYear * 10000 + SMonth * 100 + SDate = ?",
    )
    .bind(&f.group)
    .bind(&f.pagi)
    .bind(
        day(0)
            .format("%Y%m%d")
            .to_string()
            .parse::<i32>()
            .expect("key"),
    )
    .fetch_one(&f.app.state.db)
    .await
    .expect("count");
    assert_eq!(today_pagi, 1);
}
