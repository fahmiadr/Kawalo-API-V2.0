//! `/api/v1/information` against the scratch MariaDB and Redis. Every test owns a random group and
//! random users, so they run in parallel without interfering.

mod common;

use axum::http::StatusCode;
use common::TestApp;
use kawalo_api::core::security::{JwtUserPayload, sign_access_token};
use serde_json::{Value, json};
use uuid::Uuid;

const URL: &str = "/api/v1/information";

struct Tenant {
    app: TestApp,
    group: String,
    admin: String,
    guard: String,
    other_guard: String,
    resident: String,
}

fn unique(prefix: &str) -> String {
    format!("{prefix}-{}", Uuid::new_v4().simple())
}

impl Tenant {
    async fn new() -> Self {
        let app = TestApp::start().await;
        let group = unique("grp");
        let admin = format!("{}@info.test", unique("admin"));
        let guard = format!("{}@info.test", unique("guard"));
        let other_guard = format!("{}@info.test", unique("guard"));
        let resident = format!("{}@info.test", unique("res"));
        let admin = app.token_for(&admin, &group, true).await;
        let guard = app.token_for(&guard, &group, false).await;
        let other_guard = app.token_for(&other_guard, &group, false).await;
        let resident = role_token(&app, &resident, &group, "USER").await;
        Self {
            group,
            admin,
            guard,
            other_guard,
            resident,
            app,
        }
    }

    async fn call(
        &self,
        method: &str,
        path: &str,
        token: Option<&str>,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        self.app.call(method, path, token, body.as_ref()).await
    }

    /// Creates a feed and returns its id.
    async fn create(&self, token: &str, body: Value) -> String {
        let (status, json) = self.call("POST", URL, Some(token), Some(body)).await;
        assert_eq!(status, StatusCode::CREATED, "{json}");
        json["data"]["id"].as_str().expect("id").to_owned()
    }

    async fn titles(&self, token: &str, query: &str) -> Vec<String> {
        let (status, json) = self
            .call("GET", &format!("{URL}{query}"), Some(token), None)
            .await;
        assert_eq!(status, StatusCode::OK, "{json}");
        json["data"]
            .as_array()
            .expect("list")
            .iter()
            .map(|item| item["title"].as_str().unwrap_or_default().to_owned())
            .collect()
    }

    /// Inserts a legacy-style row straight into the table.
    async fn insert_raw(&self, columns: &str, values: &str) {
        let sql = format!(
            "INSERT INTO tkfeed (FeedUID, FeedGroupUID, {columns}) VALUES ('{}', '{}', {values})",
            unique("feed"),
            self.group
        );
        let mut conn = self.app.state.db.acquire().await.expect("connection");
        sqlx::query("SET SESSION sql_mode=''")
            .execute(&mut *conn)
            .await
            .expect("sql_mode");
        sqlx::query(sqlx::AssertSqlSafe(sql))
            .execute(&mut *conn)
            .await
            .expect("insert");
    }
}

async fn role_token(app: &TestApp, email: &str, group: &str, role: &str) -> String {
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
        session_ver,
        ..JwtUserPayload::default()
    };
    sign_access_token(&app.state.keys, app.state.config.jwt_expires_in, &payload)
        .expect("sign")
        .token
}

fn assert_error(result: &(StatusCode, Value), status: StatusCode, code: &str, message: &str) {
    let (got_status, body) = result;
    assert_eq!(*got_status, status, "{body}");
    assert_eq!(body["success"], false, "{body}");
    assert_eq!(body["error_code"], code, "{body}");
    assert_eq!(body["message"], message, "{body}");
}

#[tokio::test]
async fn create_returns_the_full_item_and_lists_it_for_the_group_only() {
    let t = Tenant::new().await;
    let (status, json) = t
        .call(
            "POST",
            URL,
            Some(&t.admin),
            Some(json!({
                "title": "  Kerja Bakti  ", "text": "Isi", "feed_file": "https://x/f.pdf",
                "photos": ["", "https://x/1.jpg", "N/A", "https://x/2.jpg"],
                "feed_type": "EVENT", "event_location": "Balai", "glat": "-6.2", "glong": "106.8",
            })),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{json}");
    assert_eq!(json["success"], true);
    assert_eq!(json["message"], "Data informasi berhasil dibuat");
    let data = &json["data"];
    assert!(
        data["id"]
            .as_str()
            .is_some_and(|id| id.parse::<u32>().is_ok())
    );
    assert_eq!(data["title"], "Kerja Bakti");
    assert_eq!(data["text"], "Isi");
    assert_eq!(
        data["photos"],
        json!(["https://x/1.jpg", "https://x/2.jpg"])
    );
    assert_eq!(data["feed_type"], "EVENT");
    assert_eq!(data["event_location"], "Balai");
    assert_eq!(data["glat"], "-6.2");
    assert_eq!(data["glong"], "106.8");
    assert_eq!(data["feed_file"], "https://x/f.pdf");
    assert!(
        data["user_email"]
            .as_str()
            .is_some_and(|e| e.ends_with("@info.test"))
    );
    assert!(
        data["user_name"]
            .as_str()
            .is_some_and(|n| n.starts_with("admin-"))
    );
    assert!(data["feed_dt"].as_str().is_some_and(|d| d.ends_with('Z')));
    assert_eq!(data["event_duration"], Value::Null);
    let keys: Vec<&str> = data
        .as_object()
        .expect("object")
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        keys,
        [
            "id",
            "title",
            "text",
            "user_email",
            "user_name",
            "user_profile_pic",
            "feed_dt",
            "feed_file",
            "photos",
            "feed_type",
            "group_name",
            "event_location",
            "event_start_dt",
            "event_end_dt",
            "event_duration",
            "glat",
            "glong"
        ]
    );

    let (status, list) = t.call("GET", URL, Some(&t.guard), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list["message"], "Daftar data informasi berhasil diambil");
    assert_eq!(list["data"].as_array().map(Vec::len), Some(1));
    assert_eq!(list["data"][0], *data);

    let stranger = role_token(
        &t.app,
        &format!("{}@info.test", unique("s")),
        &unique("grp"),
        "ADMIN",
    )
    .await;
    assert!(t.titles(&stranger, "").await.is_empty());
}

#[tokio::test]
async fn create_defaults_and_cleans_optional_fields() {
    let t = Tenant::new().await;
    let id = t
        .create(
            &t.guard,
            json!({
                "title": "Minimal", "text": "N/A", "feed_type": null, "glat": "0", "glong": "abc",
                "event_location": "  Taman ", "photos": [],
            }),
        )
        .await;
    let (_, list) = t.call("GET", URL, Some(&t.guard), None).await;
    let item = &list["data"][0];
    assert_eq!(item["id"], id);
    assert_eq!(item["feed_type"], "OTHER");
    assert_eq!(item["text"], Value::Null);
    assert_eq!(item["glat"], Value::Null);
    assert_eq!(item["glong"], Value::Null);
    assert_eq!(item["event_location"], "Taman");
    assert_eq!(item["photos"], json!([]));
}

#[tokio::test]
async fn create_takes_group_name_and_profile_pic_from_the_account() {
    let t = Tenant::new().await;
    let email = format!("{}@info.test", unique("pic"));
    let token = role_token(&t.app, &email, &t.group, "ADMIN").await;
    let group_name = unique("Grup");
    sqlx::query("INSERT INTO tkgroup (GroupUID, GroupName, GroupOwner) VALUES (?, ?, 'o@x.id')")
        .bind(&t.group)
        .bind(&group_name)
        .execute(&t.app.state.db)
        .await
        .expect("group");
    sqlx::query(
        "INSERT INTO tkuser (UserUID, UserEmail, UserName, Password, UserProfilePic) \
         VALUES (?, ?, 'Nama Akun', 'x', 'pic.png')",
    )
    .bind(unique("u"))
    .bind(&email)
    .execute(&t.app.state.db)
    .await
    .expect("user");
    let (_, json) = t
        .call(
            "POST",
            URL,
            Some(&token),
            Some(json!({"title": "Dengan akun"})),
        )
        .await;
    assert_eq!(json["data"]["group_name"], group_name.as_str());
    assert_eq!(json["data"]["user_profile_pic"], "pic.png");
}

#[tokio::test]
async fn legacy_rows_fall_back_to_the_account_and_drop_dirty_values() {
    let t = Tenant::new().await;
    let email = format!("{}@info.test", unique("legacy"));
    sqlx::query(
        "INSERT INTO tkuser (UserUID, UserEmail, UserName, Password, UserProfilePic) \
         VALUES (?, ?, 'Nama Dari Akun', 'x', 'akun.png')",
    )
    .bind(unique("u"))
    .bind(&email)
    .execute(&t.app.state.db)
    .await
    .expect("user");
    let group_name = unique("Grup");
    sqlx::query("INSERT INTO tkgroup (GroupUID, GroupName, GroupOwner) VALUES (?, ?, 'o@x.id')")
        .bind(&t.group)
        .bind(&group_name)
        .execute(&t.app.state.db)
        .await
        .expect("group");
    t.insert_raw(
        "FeedTitle, FeedByEmail, FeedByName, FeedByProfilePic, FeedGroupName, FeedDT, \
         FeedEventStartDT, FeedEventEndDT, FeedPic, FeedPic2, FeedPic3, FeedPic4, GLat, GLong, \
         FeedText, FeedType",
        &format!(
            "'Lama', '  {}  ', 'N/A', NULL, NULL, '0000-00-00 00:00:00', '0000-00-00 00:00:00', \
             '1970-01-01 00:00:01', 'N/A', 'a.jpg', 'null', 'b.jpg', '0.000000', 'x', '  ', 'general'",
            email.to_uppercase()
        ),
    )
    .await;
    let (status, list) = t.call("GET", URL, Some(&t.admin), None).await;
    assert_eq!(status, StatusCode::OK, "{list}");
    let item = &list["data"][0];
    assert_eq!(item["user_email"], email.as_str());
    assert_eq!(item["user_name"], "Nama Dari Akun");
    assert_eq!(item["user_profile_pic"], "akun.png");
    assert_eq!(item["group_name"], group_name.as_str());
    assert_eq!(item["feed_dt"], Value::Null);
    assert_eq!(item["event_start_dt"], Value::Null);
    assert_eq!(item["event_end_dt"], Value::Null);
    assert_eq!(item["photos"], json!(["a.jpg", "b.jpg"]));
    assert_eq!(item["glat"], Value::Null);
    assert_eq!(item["glong"], Value::Null);
    assert_eq!(item["text"], Value::Null);
    assert_eq!(item["feed_type"], "general");
}

#[tokio::test]
async fn create_failures() {
    let t = Tenant::new().await;
    let post = |token: Option<&str>, body: Option<Value>| {
        let token = token.map(str::to_owned);
        let t = &t;
        async move { t.call("POST", URL, token.as_deref(), body).await }
    };

    for title in ["   ", "N/A", "null"] {
        assert_error(
            &post(Some(&t.admin), Some(json!({"title": title}))).await,
            StatusCode::BAD_REQUEST,
            "VALIDATION_ERROR",
            "Judul informasi wajib diisi.",
        );
    }
    assert_error(
        &post(Some(&t.resident), Some(json!({"title": "x"}))).await,
        StatusCode::FORBIDDEN,
        "AUTH_FORBIDDEN",
        "Anda tidak memiliki wewenang untuk mengakses sumber daya ini.",
    );
    assert_error(
        &post(None, Some(json!({"title": "x"}))).await,
        StatusCode::UNAUTHORIZED,
        "AUTH_UNAUTHORIZED",
        "Akses tidak diizinkan. Token tidak valid atau telah kedaluwarsa.",
    );
    let no_group = {
        let email = format!("{}@info.test", unique("ng"));
        let session_ver = t
            .app
            .state
            .sessions
            .get_or_init_version(&email)
            .await
            .expect("ver");
        let payload = JwtUserPayload {
            sub: Some(email.clone()),
            email: Some(email),
            name: Some("Tanpa Grup".to_owned()),
            role: Some("ADMIN".to_owned()),
            session_ver,
            ..JwtUserPayload::default()
        };
        sign_access_token(
            &t.app.state.keys,
            t.app.state.config.jwt_expires_in,
            &payload,
        )
        .expect("sign")
        .token
    };
    assert_error(
        &post(Some(&no_group), Some(json!({"title": "x"}))).await,
        StatusCode::BAD_REQUEST,
        "VALIDATION_ERROR",
        "Pengguna belum memilih grup aktif.",
    );
    let (status, list) = t.call("GET", URL, Some(&no_group), None).await;
    assert_eq!((status, &list["data"]), (StatusCode::OK, &json!([])));

    for body in [
        json!({}),
        json!({"title": ""}),
        json!({"title": "t".repeat(129)}),
        json!({"title": 5}),
        json!({"title": "x", "text": "x".repeat(1025)}),
        json!({"title": "x", "photos": ["a", "b", "c", "d", "e"]}),
        json!({"title": "x", "photos": null}),
        json!({"title": "x", "feed_type": "general"}),
        json!({"title": "x", "glat": -6.2}),
    ] {
        let result = post(Some(&t.admin), Some(body.clone())).await;
        assert_error(
            &result,
            StatusCode::UNPROCESSABLE_ENTITY,
            "VALIDATION_ERROR",
            "Format data masukan tidak valid",
        );
        // Validation runs before authentication, so the answer does not depend on the token.
        assert_eq!(
            post(None, Some(body)).await.0,
            StatusCode::UNPROCESSABLE_ENTITY
        );
    }
    assert_eq!(
        post(Some(&t.admin), None).await.0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        post(Some(&t.admin), Some(Value::Null)).await.0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
}

#[tokio::test]
async fn list_filters_search_dates_and_paging() {
    let t = Tenant::new().await;
    let rows = [
        ("Rapat Warga", "2026-03-01 08:00:00", "a@info.test"),
        ("Listrik Padam", "2026-03-02 08:00:00", "b@info.test"),
        ("100% Hadir", "2026-03-03 08:00:00", "a@info.test"),
        ("Jadwal Ronda", "2026-03-04 08:00:00", "b@info.test"),
    ];
    for (title, dt, email) in rows {
        t.insert_raw(
            "FeedTitle, FeedByEmail, FeedDT",
            &format!("'{title}', '{email}', '{dt}'"),
        )
        .await;
    }
    let all = ["Jadwal Ronda", "100% Hadir", "Listrik Padam", "Rapat Warga"];
    assert_eq!(t.titles(&t.admin, "").await, all);
    assert_eq!(t.titles(&t.resident, "?limit=0&offset=3").await, all);
    assert_eq!(t.titles(&t.admin, "?limit=2").await, &all[..2]);
    assert_eq!(t.titles(&t.admin, "?limit=2&offset=2").await, &all[2..]);
    assert_eq!(t.titles(&t.admin, "?limit=0x2").await, &all[..2]);
    assert_eq!(t.titles(&t.admin, "?limit=%202%20").await, &all[..2]);
    assert_eq!(t.titles(&t.admin, "?limit=1e1").await, all);
    assert_eq!(t.titles(&t.admin, "?limit=1&limit=3").await, &all[..3]);
    assert_eq!(t.titles(&t.admin, "?limit=2.5").await, &all[..2]);

    assert_eq!(
        t.titles(&t.admin, "?search=LISTRIK").await,
        ["Listrik Padam"]
    );
    assert_eq!(t.titles(&t.admin, "?search=100%25").await, ["100% Hadir"]);
    assert_eq!(t.titles(&t.admin, "?search=R_pat").await, ["Rapat Warga"]);
    assert_eq!(
        t.titles(&t.admin, "?search=b@info").await,
        ["Jadwal Ronda", "Listrik Padam"]
    );
    assert_eq!(t.titles(&t.admin, "?search=N/A").await, all);
    assert_eq!(t.titles(&t.admin, "?search=%20%20").await, all);
    assert_eq!(
        t.titles(&t.admin, "?created_by=A@INFO.TEST").await,
        ["100% Hadir", "Rapat Warga"]
    );
    assert!(t.titles(&t.admin, "?created_by=a").await.is_empty());
    assert_eq!(
        t.titles(
            &t.admin,
            "?start_date=2026-03-02T00:00:00Z&end_date=2026-03-03T23:59:59Z"
        )
        .await,
        ["100% Hadir", "Listrik Padam"]
    );
    assert_eq!(
        t.titles(&t.admin, "?start_date=2026-03-04").await,
        ["Jadwal Ronda"]
    );
    assert_eq!(
        t.titles(&t.admin, "?start_date=nonsense&end_date=0000-00-00")
            .await,
        all
    );

    for query in [
        "?limit=abc",
        "?limit=",
        "?limit=-1",
        "?limit=101",
        "?offset=-1",
        "?offset=x",
        "?search=",
        "?created_by=",
    ] {
        let (status, body) = t
            .call("GET", &format!("{URL}{query}"), Some(&t.admin), None)
            .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{query}: {body}");
        assert_eq!(body["message"], "Format data masukan tidak valid");
        let (status, _) = t.call("GET", &format!("{URL}{query}"), None, None).await;
        assert_eq!(
            status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{query} without token"
        );
    }
    let (status, body) = t.call("GET", URL, None, None).await;
    assert_error(
        &(status, body),
        StatusCode::UNAUTHORIZED,
        "AUTH_UNAUTHORIZED",
        "Akses tidak diizinkan. Token tidak valid atau telah kedaluwarsa.",
    );
}

#[tokio::test]
async fn put_and_patch_update_only_the_fields_sent() {
    let t = Tenant::new().await;
    for method in ["PUT", "PATCH"] {
        let id = t
            .create(
                &t.guard,
                json!({
                    "title": "Awal", "text": "Teks", "feed_type": "GENERAL", "event_location": "Pos",
                    "photos": ["https://x/1.jpg", "https://x/2.jpg"], "glat": "-6.1",
                    "glong": "106.1", "feed_file": "https://x/f.pdf",
                }),
            )
            .await;
        let path = format!("{URL}/{id}");
        let (status, json) = t
            .call(
                method,
                &path,
                Some(&t.guard),
                Some(json!({"title": " Baru ", "feed_type": "JOIN"})),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{json}");
        assert_eq!(json["message"], "Data informasi berhasil diperbarui");
        assert_eq!(json["data"]["title"], "Baru");
        assert_eq!(json["data"]["feed_type"], "JOIN");
        assert_eq!(json["data"]["text"], "Teks");
        assert_eq!(
            json["data"]["photos"],
            json!(["https://x/1.jpg", "https://x/2.jpg"])
        );

        // `null` and blank titles and a `null` feed_type are ignored rather than cleared.
        let (_, json) = t
            .call(
                method,
                &path,
                Some(&t.guard),
                Some(json!({"title": null, "feed_type": null})),
            )
            .await;
        assert_eq!(
            (&json["data"]["title"], &json["data"]["feed_type"]),
            (&json!("Baru"), &json!("JOIN"))
        );
        let (_, json) = t
            .call(method, &path, Some(&t.guard), Some(json!({"title": "   "})))
            .await;
        assert_eq!(json["data"]["title"], "Baru");

        // Other nullable fields are cleared by `null`, blanks and zero coordinates.
        let (_, json) = t
            .call(
                method,
                &path,
                Some(&t.guard),
                Some(json!({"text": null, "event_location": "  ", "glat": "0", "glong": null, "feed_file": null})),
            )
            .await;
        let data = &json["data"];
        for field in ["text", "event_location", "glat", "glong", "feed_file"] {
            assert_eq!(data[field], Value::Null, "{method} {field}");
        }
        assert_eq!(
            data["photos"],
            json!(["https://x/1.jpg", "https://x/2.jpg"])
        );

        let (_, json) = t
            .call(
                method,
                &path,
                Some(&t.guard),
                Some(json!({"photos": ["", "https://x/n.jpg", "N/A"]})),
            )
            .await;
        assert_eq!(json["data"]["photos"], json!(["https://x/n.jpg"]));
        let (_, json) = t
            .call(method, &path, Some(&t.guard), Some(json!({"photos": []})))
            .await;
        assert_eq!(json["data"]["photos"], json!([]));

        let (status, json) = t.call(method, &path, Some(&t.guard), Some(json!({}))).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(json["data"]["title"], "Baru");

        let (_, json) = t
            .call(
                method,
                &path,
                Some(&t.guard),
                Some(json!({"text": "Kembali", "glat": "-6.5", "feed_type": "EVENT"})),
            )
            .await;
        assert_eq!(json["data"]["text"], "Kembali");
        assert_eq!(json["data"]["glat"], "-6.5");
        assert_eq!(json["data"]["feed_type"], "EVENT");
    }
}

#[tokio::test]
async fn update_authorisation_and_failures() {
    let t = Tenant::new().await;
    let own = t.create(&t.guard, json!({"title": "Milik guard"})).await;
    let body = || Some(json!({"title": "Diubah"}));
    let put = |id: &str, token: Option<&str>, body: Option<Value>| {
        let path = format!("{URL}/{id}");
        let token = token.map(str::to_owned);
        let t = &t;
        async move { t.call("PUT", &path, token.as_deref(), body).await }
    };

    assert_error(
        &put(&own, Some(&t.other_guard), body()).await,
        StatusCode::FORBIDDEN,
        "AUTH_FORBIDDEN",
        "Petugas (Guard) hanya dapat mengubah informasi miliknya sendiri.",
    );
    assert_eq!(put(&own, Some(&t.admin), body()).await.0, StatusCode::OK);
    assert_error(
        &put(&own, Some(&t.resident), body()).await,
        StatusCode::FORBIDDEN,
        "AUTH_FORBIDDEN",
        "Anda tidak memiliki wewenang untuk mengakses sumber daya ini.",
    );
    for id in ["99999999", "0", "99999999999999999999"] {
        assert_error(
            &put(id, Some(&t.admin), body()).await,
            StatusCode::NOT_FOUND,
            "RESOURCE_NOT_FOUND",
            "Data informasi tidak ditemukan.",
        );
    }
    let foreign = role_token(
        &t.app,
        &format!("{}@info.test", unique("f")),
        &unique("grp"),
        "ADMIN",
    )
    .await;
    assert_eq!(
        put(&own, Some(&foreign), body()).await.0,
        StatusCode::NOT_FOUND
    );

    for id in ["abc", "%201", "1.5"] {
        assert_eq!(
            put(id, None, body()).await.0,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{id}"
        );
    }
    for bad in [
        json!({"title": ""}),
        json!({"feed_type": "general"}),
        json!({"photos": null}),
        json!({"photos": ["a", "b", "c", "d", "e"]}),
    ] {
        assert_eq!(
            put(&own, Some(&t.admin), Some(bad)).await.0,
            StatusCode::UNPROCESSABLE_ENTITY
        );
    }
    assert_error(
        &put(&own, None, body()).await,
        StatusCode::UNAUTHORIZED,
        "AUTH_UNAUTHORIZED",
        "Akses tidak diizinkan. Token tidak valid atau telah kedaluwarsa.",
    );
    assert_eq!(
        put(&own, Some(&t.admin), None).await.0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
}

#[tokio::test]
async fn delete_single_enforces_group_and_ownership() {
    let t = Tenant::new().await;
    let mine = t.create(&t.guard, json!({"title": "Hapus saya"})).await;
    let theirs = t
        .create(&t.other_guard, json!({"title": "Bukan milik"}))
        .await;
    let delete = |id: &str, token: Option<&str>| {
        let path = format!("{URL}/{id}");
        let token = token.map(str::to_owned);
        let t = &t;
        async move { t.call("DELETE", &path, token.as_deref(), None).await }
    };

    assert_error(
        &delete(&theirs, Some(&t.guard)).await,
        StatusCode::FORBIDDEN,
        "AUTH_FORBIDDEN",
        "Petugas (Guard) hanya dapat menghapus informasi miliknya sendiri.",
    );
    assert_eq!(
        delete(&mine, Some(&t.resident)).await.0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        delete("abc", None).await.0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(delete(&mine, None).await.0, StatusCode::UNAUTHORIZED);

    let (status, json) = delete(&mine, Some(&t.guard)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["message"], "Data informasi berhasil dihapus");
    assert_eq!(json["data"], Value::Null);
    assert_error(
        &delete(&mine, Some(&t.guard)).await,
        StatusCode::NOT_FOUND,
        "RESOURCE_NOT_FOUND",
        "Data informasi tidak ditemukan.",
    );
    assert_eq!(delete(&theirs, Some(&t.admin)).await.0, StatusCode::OK);
    assert_eq!(delete("0", Some(&t.admin)).await.0, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn delete_batch_is_all_or_nothing() {
    let t = Tenant::new().await;
    let a = t.create(&t.guard, json!({"title": "A"})).await;
    let b = t.create(&t.guard, json!({"title": "B"})).await;
    let c = t.create(&t.other_guard, json!({"title": "C"})).await;
    let batch = |ids: Value, token: Option<&str>| {
        let token = token.map(str::to_owned);
        let t = &t;
        async move {
            t.call("DELETE", URL, token.as_deref(), Some(json!({ "ids": ids })))
                .await
        }
    };

    for ids in [json!([]), json!(["x"]), json!(["1", ""]), json!([1])] {
        assert_eq!(batch(ids, None).await.0, StatusCode::UNPROCESSABLE_ENTITY);
    }
    assert_eq!(batch(json!([a]), None).await.0, StatusCode::UNAUTHORIZED);
    assert_eq!(
        batch(json!([a]), Some(&t.resident)).await.0,
        StatusCode::FORBIDDEN
    );
    assert_error(
        &batch(json!([a, "99999999"]), Some(&t.admin)).await,
        StatusCode::NOT_FOUND,
        "RESOURCE_NOT_FOUND",
        "Satu atau lebih data informasi tidak ditemukan dalam grup aktif.",
    );
    assert_error(
        &batch(json!(["99999999", "0"]), Some(&t.admin)).await,
        StatusCode::NOT_FOUND,
        "RESOURCE_NOT_FOUND",
        "Data informasi tidak ditemukan.",
    );
    // `01` and `1` are different ids for the same row, so the counts disagree.
    assert_eq!(
        batch(json!([a.clone(), format!("0{a}")]), Some(&t.admin))
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    assert_error(
        &batch(json!([a, c]), Some(&t.guard)).await,
        StatusCode::FORBIDDEN,
        "AUTH_FORBIDDEN",
        "Petugas (Guard) hanya dapat menghapus informasi miliknya sendiri.",
    );
    assert_eq!(t.titles(&t.admin, "").await.len(), 3, "nothing was deleted");

    let (status, json) = batch(json!([a.clone(), a, b]), Some(&t.guard)).await;
    assert_eq!(status, StatusCode::OK, "{json}");
    assert_eq!(json["message"], "2 data informasi berhasil dihapus");
    assert_eq!(json["data"], json!({"deleted_count": 2}));
    assert_eq!(t.titles(&t.admin, "").await, ["C"]);
    assert_eq!(
        batch(json!([c]), Some(&t.admin)).await.1["data"]["deleted_count"],
        1
    );
}

#[tokio::test]
async fn unsupported_methods_are_not_found() {
    let t = Tenant::new().await;
    for (method, path) in [
        ("GET", format!("{URL}/1")),
        ("PUT", URL.to_owned()),
        ("PATCH", URL.to_owned()),
    ] {
        let (status, body) = t.call(method, &path, Some(&t.admin), Some(json!({}))).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{method} {path}: {body}");
    }
}
