//! `/api/v1/guests` against the scratch MariaDB and Redis; expectations are derived from the
//! TypeScript implementation and its unit tests.

mod common;

use axum::http::StatusCode;
use common::TestApp;
use kawalo_api::core::security::{JwtUserPayload, sign_access_token};
use serde_json::{Value, json};
use uuid::Uuid;

const GUEST_KEYS: [&str; 22] = [
    "id",
    "group_name",
    "guest_name",
    "guest_number",
    "guest_identity",
    "guest_identity_type",
    "guest_photo",
    "guest_dt",
    "guest_ack_dt",
    "guest_out_dt",
    "guest_purpose",
    "guest_destination",
    "guest_to_user_email",
    "guest_to_user_name",
    "guest_creator_email",
    "guest_creator_name",
    "guest_category",
    "qr_uid",
    "guest_address",
    "guest_type",
    "guest_checkout_by_email",
    "guest_checkout_by_name",
];

/// An isolated tenant: one group with an admin, two guards, a resident host and a plain member.
struct Tenant {
    group: String,
    admin: String,
    guard: String,
    other_guard: String,
    host: String,
    /// Registered guest type: `(uid, name)`.
    guest_type: (String, String),
}

impl Tenant {
    async fn new(app: &TestApp) -> Self {
        let id = Uuid::new_v4().simple().to_string();
        let tenant = Self {
            group: format!("grp-{id}"),
            admin: format!("admin-{id}@t.id"),
            guard: format!("guard-{id}@t.id"),
            other_guard: format!("guard2-{id}@t.id"),
            host: format!("host-{id}@t.id"),
            guest_type: (format!("gt-{id}"), format!("Tamu {id}")),
        };
        let db = &app.state.db;
        sqlx::query(
            "INSERT INTO tkgroup (GroupUID, GroupName, GroupOwner) VALUES (?, ?, 'o@t.id')",
        )
        .bind(&tenant.group)
        .bind(format!("Grup {id}"))
        .execute(db)
        .await
        .expect("group");
        for (email, name, category) in [
            (&tenant.admin, "Admin Gedung", "ADMIN"),
            (&tenant.guard, "Satpam Satu", "GUARD"),
            (&tenant.other_guard, "Satpam Dua", "GUARD"),
            (&tenant.host, "Budi Penghuni", "USER"),
        ] {
            sqlx::query(
                "INSERT INTO tkuser (UserUID, UserEmail, UserName, Password) VALUES (?, ?, ?, 'x')",
            )
            .bind(format!("u-{}", Uuid::new_v4()))
            .bind(email)
            .bind(name)
            .execute(db)
            .await
            .expect("user");
            sqlx::query(
                "INSERT INTO tkusergroup (Email, GroupUID, Category, Status) VALUES (?, ?, ?, 'JOINED')",
            )
            .bind(email)
            .bind(&tenant.group)
            .bind(category)
            .execute(db)
            .await
            .expect("membership");
        }
        sqlx::query(
            "INSERT INTO tkmasterguesttype (GuestTypeUID, GuestTypeGroupUID, GuestTypeName) \
             VALUES (?, ?, ?), (?, ?, 'Kurir')",
        )
        .bind(&tenant.guest_type.0)
        .bind(&tenant.group)
        .bind(&tenant.guest_type.1)
        .bind(format!("kurir-{id}"))
        .bind(&tenant.group)
        .execute(db)
        .await
        .expect("guest types");
        tenant
    }

    async fn token(&self, app: &TestApp, email: &str) -> String {
        let role = if email == self.admin {
            "ADMIN"
        } else if email == self.host {
            "USER"
        } else {
            "GUARD"
        };
        token_with(app, email, Some(&self.group), Some(role)).await
    }

    /// Inserts a guest row directly and returns `(GuestId, QRUID)`.
    async fn seed(&self, app: &TestApp, name: &str, creator: &str, arrived: &str) -> (i64, String) {
        let qr = Uuid::new_v4().to_string();
        let result = sqlx::query(
            "INSERT INTO tkguest (GuestUID, GuestGroupUID, GuestName, GuestDestination, GuestType, \
             GuestCreatorEmail, GuestCreatorName, GuestDT, QRUID, GuestCategory) \
             VALUES (?, ?, ?, 'Blok A', ?, ?, 'Pembuat', ?, ?, 'Umum')",
        )
        .bind(format!("guest_{}", Uuid::new_v4()))
        .bind(&self.group)
        .bind(name)
        .bind(&self.guest_type.1)
        .bind(creator)
        .bind(arrived)
        .bind(&qr)
        .execute(&app.state.db)
        .await
        .expect("seed guest");
        (i64::try_from(result.last_insert_id()).expect("id"), qr)
    }
}

async fn token_with(app: &TestApp, email: &str, group: Option<&str>, role: Option<&str>) -> String {
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
        role: role.map(str::to_owned),
        group_uid: group.map(str::to_owned),
        session_ver,
        ..JwtUserPayload::default()
    };
    sign_access_token(&app.state.keys, app.state.config.jwt_expires_in, &payload)
        .expect("sign")
        .token
}

fn assert_error(response: &(StatusCode, Value), status: u16, code: &str, message: &str) {
    let (got_status, body) = response;
    assert_eq!(got_status.as_u16(), status, "{body}");
    assert_eq!(body["success"], false, "{body}");
    assert_eq!(body["error_code"], code, "{body}");
    assert_eq!(body["message"], message, "{body}");
}

/// serde's own messages carry a trailing position, so detail text is compared by prefix.
fn assert_details(body: &Value, expected: &str) {
    let details = body["details"].as_str().unwrap_or_default();
    assert!(details.starts_with(expected), "{details:?} vs {expected:?}");
}

fn names(body: &Value) -> Vec<&str> {
    body["data"]
        .as_array()
        .expect("array data")
        .iter()
        .map(|g| g["guest_name"].as_str().expect("name"))
        .collect()
}

#[tokio::test]
async fn auth_and_validation_order_follow_elysia() {
    let app = TestApp::start().await;
    let t = Tenant::new(&app).await;
    let valid = json!({"guest_name": "A", "guest_destination": "B", "guest_type": "Kurir"});

    let r = app.call("POST", "/api/v1/guests", None, Some(&valid)).await;
    assert_error(
        &r,
        401,
        "AUTH_UNAUTHORIZED",
        "Akses tidak diizinkan. Token tidak valid atau telah kedaluwarsa.",
    );

    // The body schema is checked before the auth macros.
    let r = app
        .call("POST", "/api/v1/guests", None, Some(&json!({})))
        .await;
    assert_eq!(r.0, StatusCode::UNPROCESSABLE_ENTITY, "{}", r.1);
    assert_eq!(r.1["message"], "Format data masukan tidak valid");
    assert_eq!(r.1["error_code"], "VALIDATION_ERROR");

    for (method, path) in [
        ("GET", "/api/v1/guests"),
        ("GET", "/api/v1/guests/master-types"),
        ("GET", "/api/v1/guests/1"),
    ] {
        let r = app.call(method, path, None, None).await;
        assert_error(
            &r,
            401,
            "AUTH_UNAUTHORIZED",
            "Akses tidak diizinkan. Token tidak valid atau telah kedaluwarsa.",
        );
    }

    let host = t.token(&app, &t.host).await;
    for (method, path, body) in [
        ("POST", "/api/v1/guests", Some(&valid)),
        ("PUT", "/api/v1/guests/1", Some(&json!({}))),
        ("PATCH", "/api/v1/guests/1", Some(&json!({}))),
        ("PATCH", "/api/v1/guests/1/checkout", Some(&json!({}))),
        ("DELETE", "/api/v1/guests/1", None),
        ("DELETE", "/api/v1/guests", Some(&json!({"ids": ["1"]}))),
    ] {
        let r = app.call(method, path, Some(&host), body).await;
        assert_error(
            &r,
            403,
            "AUTH_FORBIDDEN",
            "Anda tidak memiliki wewenang untuk mengakses sumber daya ini.",
        );
    }

    // Path parameters are validated before auth too.
    let r = app.call("GET", "/api/v1/guests/a.b", None, None).await;
    assert_eq!(r.0, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        r.1["details"],
        "/id: Expected string to match '^[a-zA-Z0-9_-]+$'"
    );
    let long = "a".repeat(65);
    let r = app
        .call("GET", &format!("/api/v1/guests/{long}"), None, None)
        .await;
    assert_eq!(r.0, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn master_types_are_group_scoped_and_sorted() {
    let app = TestApp::start().await;
    let t = Tenant::new(&app).await;
    let other = Tenant::new(&app).await;
    let token = t.token(&app, &t.guard).await;

    let (status, body) = app
        .call("GET", "/api/v1/guests/master-types", Some(&token), None)
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["message"], "Daftar tipe tamu master berhasil diambil");
    let data = body["data"].as_array().expect("list");
    assert_eq!(data.len(), 2, "{body}");
    assert_eq!(data[0]["name"], "Kurir");
    assert_eq!(data[1]["name"], t.guest_type.1.as_str());
    assert_eq!(data[1]["uid"], t.guest_type.0.as_str());
    assert!(data[1]["id"].is_string());
    assert!(!body.to_string().contains(&other.guest_type.1));

    let nogroup = token_with(&app, &t.guard, None, Some("GUARD")).await;
    let (status, body) = app
        .call("GET", "/api/v1/guests/master-types", Some(&nogroup), None)
        .await;
    assert_eq!((status, &body["data"]), (StatusCode::OK, &json!([])));
}

#[tokio::test]
async fn create_validates_and_returns_the_full_guest() {
    let app = TestApp::start().await;
    let t = Tenant::new(&app).await;
    let guard = t.token(&app, &t.guard).await;
    let post = |body: Value| {
        let (app, token) = (&app, &guard);
        async move {
            app.call("POST", "/api/v1/guests", Some(token), Some(&body))
                .await
        }
    };
    let base = json!({"guest_name": "Tamu Baru", "guest_destination": "Blok B", "guest_type": t.guest_type.0});

    for (patch, detail) in [
        (
            json!({"guest_name": ""}),
            "/guest_name: Expected string length greater or equal to 1",
        ),
        (
            json!({"guest_name": "x".repeat(129)}),
            "/guest_name: Expected string length less or equal to 128",
        ),
        (
            json!({"guest_number": "x".repeat(33)}),
            "/guest_number: Expected string length less or equal to 32",
        ),
        (
            json!({"guest_identity_type": "BPJS"}),
            "/guest_identity_type: Expected one of: KTP, SIM, PASPOR, LAINNYA",
        ),
        (
            json!({"guest_to_user_email": "bukan-email"}),
            "/guest_to_user_email: Property 'guest_to_user_email' should be email",
        ),
        (
            json!({"guest_type": null}),
            "/guest_type: invalid type: null, expected a string",
        ),
    ] {
        let mut body = base.clone();
        body.as_object_mut()
            .expect("object")
            .extend(patch.as_object().expect("patch").clone());
        let r = post(body).await;
        assert_eq!(r.0, StatusCode::UNPROCESSABLE_ENTITY, "{detail}: {}", r.1);
        assert_details(&r.1, detail);
    }
    let r = post(json!({"guest_destination": "x", "guest_type": "x"})).await;
    assert_eq!(r.0, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(r.1["details"], "/guest_name: Expected required property");

    let r =
        post(json!({"guest_name": "   ", "guest_destination": "B", "guest_type": "Kurir"})).await;
    assert_error(&r, 400, "VALIDATION_ERROR", "Nama tamu wajib diisi.");
    let r =
        post(json!({"guest_name": "A", "guest_destination": "N/A", "guest_type": "Kurir"})).await;
    assert_error(&r, 400, "VALIDATION_ERROR", "Destinasi tamu wajib diisi.");
    let r =
        post(json!({"guest_name": "A", "guest_destination": "B", "guest_type": " null "})).await;
    assert_error(
        &r,
        400,
        "VALIDATION_ERROR",
        "Tipe tamu wajib diisi dan harus terdaftar pada grup aktif.",
    );
    let r = post(json!({"guest_name": "A", "guest_destination": "B", "guest_type": "Tak Dikenal"}))
        .await;
    assert_error(
        &r,
        400,
        "VALIDATION_ERROR",
        "Tipe tamu tidak valid atau tidak terdaftar pada grup aktif.",
    );
    let mut body = base.clone();
    body["guest_to_user_email"] = json!("orang.asing@t.id");
    let r = post(body).await;
    assert_error(
        &r,
        400,
        "VALIDATION_ERROR",
        "Penghuni yang dituju tidak terdaftar pada grup aktif.",
    );

    let nogroup = token_with(&app, &t.guard, None, Some("GUARD")).await;
    let r = app
        .call("POST", "/api/v1/guests", Some(&nogroup), Some(&base))
        .await;
    assert_error(
        &r,
        400,
        "VALIDATION_ERROR",
        "Pengguna belum memilih grup aktif.",
    );

    let mut body = base.clone();
    body["guest_to_user_email"] = json!(t.host.to_uppercase());
    body["guest_identity_type"] = json!("KTP");
    body["guest_purpose"] = json!("  Antar paket  ");
    body["guest_number"] = json!(null);
    body["guest_address"] = json!("N/A");
    body["guest_dt"] = json!("2025-03-04T05:06:07Z");
    let (status, created) = post(body).await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    assert_eq!(created["message"], "Data tamu berhasil dibuat");
    let data = created["data"].as_object().expect("guest");
    assert_eq!(
        data.keys().map(String::as_str).collect::<Vec<_>>(),
        GUEST_KEYS
    );
    assert_eq!(data["guest_name"], "Tamu Baru");
    assert_eq!(
        data["guest_type"],
        t.guest_type.1.as_str(),
        "stored as the canonical name"
    );
    assert_eq!(data["guest_category"], "Umum");
    assert_eq!(data["guest_purpose"], "Antar paket");
    assert_eq!(data["guest_identity_type"], "KTP");
    assert_eq!(data["guest_number"], Value::Null);
    assert_eq!(data["guest_address"], Value::Null);
    assert_eq!(data["guest_dt"], "2025-03-04T05:06:07.000Z");
    assert_eq!(data["guest_to_user_email"], t.host.as_str());
    assert_eq!(data["guest_to_user_name"], "Budi Penghuni");
    assert_eq!(data["guest_creator_email"], t.guard.as_str());
    assert_eq!(
        data["guest_creator_name"],
        t.guard.split('@').next().expect("local part")
    );
    assert_eq!(
        data["group_name"],
        format!("Grup {}", t.group.trim_start_matches("grp-"))
    );
    assert_eq!(data["qr_uid"].as_str().map(str::len), Some(36));
    for key in [
        "guest_ack_dt",
        "guest_out_dt",
        "guest_checkout_by_email",
        "guest_checkout_by_name",
    ] {
        assert_eq!(data[key], Value::Null, "{key}");
    }
    let id = data["id"].as_str().expect("string id");
    assert!(id.parse::<u32>().is_ok(), "{id}");

    // Without `guest_dt` the arrival time is now.
    let (_, body) = post(base).await;
    let arrived = body["data"]["guest_dt"].as_str().expect("dt");
    assert!(arrived.ends_with('Z') && arrived.len() == 24, "{arrived}");
}

#[tokio::test]
async fn list_filters_search_order_and_paging() {
    let app = TestApp::start().await;
    let t = Tenant::new(&app).await;
    let other = Tenant::new(&app).await;
    other
        .seed(&app, "Bocoran", &other.guard, "2026-01-05 10:00:00")
        .await;
    let (a, _) = t
        .seed(&app, "Andi Kurir", &t.guard, "2026-01-01 08:00:00")
        .await;
    let (b, _) = t
        .seed(&app, "Budi Teknisi", &t.other_guard, "2026-01-02 09:00:00")
        .await;
    t.seed(&app, "Citra Tamu", &t.guard, "2026-01-03 10:30:00")
        .await;
    sqlx::query("UPDATE tkguest SET GuestOutDT = '2026-01-01 09:00:00', GuestCheckOutByEmail = ? WHERE GuestId = ?")
        .bind(&t.guard)
        .bind(a)
        .execute(&app.state.db)
        .await
        .expect("checkout");
    sqlx::query("UPDATE tkguest SET GuestToUserEmail = ? WHERE GuestId = ?")
        .bind(&t.host)
        .bind(b)
        .execute(&app.state.db)
        .await
        .expect("host");
    let token = t.token(&app, &t.guard).await;
    let get = |query: &str| {
        let path = format!("/api/v1/guests{query}");
        let (app, token) = (&app, &token);
        async move { app.call("GET", &path, Some(token), None).await }
    };

    let (status, body) = get("").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["message"], "Daftar data tamu berhasil diambil");
    assert_eq!(
        names(&body),
        ["Citra Tamu", "Budi Teknisi", "Andi Kurir"],
        "newest first, own group only"
    );

    assert_eq!(names(&get("?limit=1").await.1), ["Citra Tamu"]);
    assert_eq!(names(&get("?limit=1&offset=1").await.1), ["Budi Teknisi"]);
    assert_eq!(names(&get("?limit=2&offset=2").await.1), ["Andi Kurir"]);
    assert_eq!(
        names(&get("?limit=0&offset=2").await.1).len(),
        3,
        "limit=0 ignores paging"
    );
    assert_eq!(names(&get("?limit=0.9").await.1).len(), 3);
    assert_eq!(names(&get("?limit=2.7").await.1).len(), 2);

    assert_eq!(
        names(&get("?is_active=true").await.1),
        ["Citra Tamu", "Budi Teknisi"]
    );
    assert_eq!(names(&get("?is_active=false").await.1), ["Andi Kurir"]);
    assert_eq!(
        names(
            &get(&format!("?created_by={}", t.other_guard.to_uppercase()))
                .await
                .1
        ),
        ["Budi Teknisi"]
    );
    assert_eq!(
        names(&get("?start_date=2026-01-02T00:00:00Z").await.1),
        ["Citra Tamu", "Budi Teknisi"]
    );
    assert_eq!(
        names(&get("?end_date=2026-01-02T09:00:00.000Z").await.1),
        ["Budi Teknisi", "Andi Kurir"]
    );
    assert_eq!(
        names(
            &get("?start_date=2026-01-02&end_date=2026-01-02T23:59:59Z")
                .await
                .1
        ),
        ["Budi Teknisi"]
    );
    assert_eq!(
        names(&get("?start_date=bukan-tanggal").await.1).len(),
        3,
        "unparsable dates are ignored"
    );
    assert_eq!(
        names(&get("?search=%20%20").await.1).len(),
        3,
        "blank search is ignored"
    );

    assert_eq!(names(&get("?search=kurir").await.1), ["Andi Kurir"]);
    assert_eq!(
        names(&get("?search=Blok%20A").await.1).len(),
        3,
        "matches destination"
    );
    assert_eq!(
        names(&get("?search=Pembuat").await.1).len(),
        3,
        "matches creator name"
    );
    assert_eq!(
        names(&get(&format!("?search={}", t.guard)).await.1),
        ["Citra Tamu", "Andi Kurir"],
        "creator and checkout email"
    );
    assert_eq!(
        names(&get("?search=Penghuni").await.1),
        ["Budi Teknisi"],
        "host name via tkuser"
    );
    assert_eq!(
        names(&get("?search=Satpam%20Satu").await.1),
        ["Andi Kurir"],
        "checkout officer name via tkuser"
    );
    assert_eq!(names(&get("?search=Bocoran").await.1), Vec::<&str>::new());
    let (_, body) = get("?limit=1&is_active=false").await;
    let guest = &body["data"][0];
    assert_eq!(guest["guest_out_dt"], "2026-01-01T09:00:00.000Z");
    assert_eq!(guest["guest_checkout_by_name"], "Satpam Satu");

    for (query, detail) in [
        (
            "?limit=101",
            "/limit: Expected number to be less or equal to 100",
        ),
        (
            "?limit=-1",
            "/limit: Expected number to be greater or equal to 0",
        ),
        (
            "?offset=-1",
            "/offset: Expected number to be greater or equal to 0",
        ),
        (
            "?search=",
            "/search: Expected string length greater or equal to 1",
        ),
        (
            "?created_by=",
            "/created_by: Expected string length greater or equal to 1",
        ),
        ("?is_active=maybe", "/is_active: Expected union value"),
    ] {
        let (status, body) = get(query).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{query}: {body}");
        assert_eq!(body["details"], detail, "{query}");
    }

    let nogroup = token_with(&app, &t.guard, None, Some("GUARD")).await;
    let (status, body) = app
        .call("GET", "/api/v1/guests", Some(&nogroup), None)
        .await;
    assert_eq!((status, &body["data"]), (StatusCode::OK, &json!([])));
}

#[tokio::test]
async fn get_by_id_accepts_numeric_id_and_qr_within_the_group() {
    let app = TestApp::start().await;
    let t = Tenant::new(&app).await;
    let other = Tenant::new(&app).await;
    let (id, qr) = t.seed(&app, "Dewi", &t.guard, "2026-01-01 08:00:00").await;
    let (foreign, foreign_qr) = other
        .seed(&app, "Asing", &other.guard, "2026-01-01 08:00:00")
        .await;
    let token = t.token(&app, &t.host).await;

    for key in [id.to_string(), qr.clone()] {
        let (status, body) = app
            .call("GET", &format!("/api/v1/guests/{key}"), Some(&token), None)
            .await;
        assert_eq!(status, StatusCode::OK, "{key}: {body}");
        assert_eq!(body["message"], "Data tamu berhasil diambil");
        assert_eq!(body["data"]["guest_name"], "Dewi");
        assert_eq!(body["data"]["id"], id.to_string());
        assert_eq!(body["data"]["qr_uid"], qr.as_str());
    }
    for key in [
        foreign.to_string(),
        foreign_qr,
        "999999999".into(),
        "tidak-ada".into(),
        "0".into(),
    ] {
        let r = app
            .call("GET", &format!("/api/v1/guests/{key}"), Some(&token), None)
            .await;
        assert_error(&r, 404, "RESOURCE_NOT_FOUND", "Data tamu tidak ditemukan.");
    }
    let r = app
        .call("GET", "/api/v1/guests/null", Some(&token), None)
        .await;
    assert_error(&r, 400, "VALIDATION_ERROR", "ID atau QR tamu wajib diisi.");

    let nogroup = token_with(&app, &t.host, None, Some("USER")).await;
    let (status, body) = app
        .call("GET", &format!("/api/v1/guests/{id}"), Some(&nogroup), None)
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"], Value::Null);
    assert_eq!(body["message"], "Data tamu berhasil diambil");
}

#[tokio::test]
async fn legacy_dirty_values_are_sanitised_on_read() {
    let app = TestApp::start().await;
    let t = Tenant::new(&app).await;
    let (id, _) = t.seed(&app, "N/A", &t.guard, "2026-01-01 08:00:00").await;
    let mut conn = app.state.db.acquire().await.expect("connection");
    sqlx::query("SET SESSION sql_mode = ''")
        .execute(&mut *conn)
        .await
        .expect("sql_mode");
    sqlx::query(
        "UPDATE tkguest SET GuestAckDT = '0000-00-00 00:00:00', GuestOutDT = '1970-01-01 00:00:01', \
         GuestPurpose = '  null ', GuestNumber = ' B 1234 ', GuestToUserEmail = 'Orang@Lama.ID', \
         GuestCreatorEmail = 'Pembuat@Lama.ID' WHERE GuestId = ?",
    )
    .bind(id)
    .execute(&mut *conn)
    .await
    .expect("dirty row");
    drop(conn);

    let token = t.token(&app, &t.admin).await;
    let (status, body) = app
        .call("GET", &format!("/api/v1/guests/{id}"), Some(&token), None)
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let guest = &body["data"];
    assert_eq!(guest["guest_name"], Value::Null);
    assert_eq!(guest["guest_ack_dt"], Value::Null, "zero date");
    assert_eq!(guest["guest_out_dt"], Value::Null, "epoch date");
    assert_eq!(guest["guest_purpose"], Value::Null);
    assert_eq!(guest["guest_number"], "B 1234");
    assert_eq!(guest["guest_to_user_email"], "orang@lama.id");
    assert_eq!(guest["guest_creator_email"], "pembuat@lama.id");
    assert_eq!(guest["guest_to_user_name"], Value::Null, "no matching user");

    // A zero acknowledge date counts as not acknowledged, an epoch checkout date as not out.
    let ack = app
        .call(
            "PATCH",
            &format!("/api/v1/guests/{id}/acknowledge"),
            Some(&token),
            Some(&json!({})),
        )
        .await;
    assert_eq!(ack.0, StatusCode::OK, "{}", ack.1);
}

#[tokio::test]
async fn update_is_tri_state_for_put_and_patch() {
    let app = TestApp::start().await;
    let t = Tenant::new(&app).await;
    let (id, qr) = t.seed(&app, "Eko", &t.guard, "2026-01-01 08:00:00").await;
    sqlx::query(
        "UPDATE tkguest SET GuestPurpose = 'Rapat', GuestNumber = 'B 1', GuestAddress = 'Jl. Lama', GuestIdentity = '123' WHERE GuestId = ?",
    )
    .bind(id)
    .execute(&app.state.db)
    .await
    .expect("fill");
    let guard = t.token(&app, &t.guard).await;
    let path = format!("/api/v1/guests/{id}");

    for method in ["PUT", "PATCH"] {
        let body = json!({
            "guest_name": "  Eko Baru ",
            "guest_purpose": null,
            "guest_number": "   ",
            "guest_destination": "N/A",
            "guest_type": t.guest_type.0,
            "guest_identity_type": "SIM",
            "guest_to_user_email": t.host.to_uppercase(),
        });
        let (status, resp) = app.call(method, &path, Some(&guard), Some(&body)).await;
        assert_eq!(status, StatusCode::OK, "{method}: {resp}");
        assert_eq!(resp["message"], "Data tamu berhasil diperbarui");
        let g = &resp["data"];
        assert_eq!(g["guest_name"], "Eko Baru");
        assert_eq!(g["guest_purpose"], Value::Null, "null clears");
        assert_eq!(g["guest_number"], Value::Null, "blank clears");
        assert_eq!(
            g["guest_destination"], "Blok A",
            "a blank required field is ignored"
        );
        assert_eq!(g["guest_address"], "Jl. Lama", "absent field stays");
        assert_eq!(g["guest_identity"], "123");
        assert_eq!(g["guest_type"], t.guest_type.1.as_str());
        assert_eq!(g["guest_identity_type"], "SIM");
        assert_eq!(g["guest_to_user_email"], t.host.as_str());
        assert_eq!(g["guest_to_user_name"], "Budi Penghuni");
        sqlx::query(
            "UPDATE tkguest SET GuestName = 'Eko', GuestToUserEmail = NULL WHERE GuestId = ?",
        )
        .bind(id)
        .execute(&app.state.db)
        .await
        .expect("reset");
    }

    let (status, resp) = app
        .call("PATCH", &path, Some(&guard), Some(&json!({})))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(resp["data"]["guest_name"], "Eko");
    let by_qr = app
        .call(
            "PUT",
            &format!("/api/v1/guests/{qr}"),
            Some(&guard),
            Some(&json!({"guest_address": null})),
        )
        .await;
    assert_eq!(by_qr.0, StatusCode::OK, "{}", by_qr.1);
    assert_eq!(by_qr.1["data"]["guest_address"], Value::Null);

    let r = app
        .call(
            "PUT",
            &path,
            Some(&guard),
            Some(&json!({"guest_type": "Tak Dikenal"})),
        )
        .await;
    assert_error(
        &r,
        400,
        "VALIDATION_ERROR",
        "Tipe tamu tidak valid atau tidak terdaftar pada grup aktif.",
    );
    let r = app
        .call(
            "PUT",
            &path,
            Some(&guard),
            Some(&json!({"guest_to_user_email": "x@t.id"})),
        )
        .await;
    assert_error(
        &r,
        400,
        "VALIDATION_ERROR",
        "Penghuni yang dituju tidak terdaftar pada grup aktif.",
    );
    let r = app
        .call(
            "PUT",
            "/api/v1/guests/987654321",
            Some(&guard),
            Some(&json!({})),
        )
        .await;
    assert_error(&r, 404, "RESOURCE_NOT_FOUND", "Data tamu tidak ditemukan.");
    let r = app
        .call("PUT", "/api/v1/guests/null", Some(&guard), Some(&json!({})))
        .await;
    assert_error(&r, 400, "VALIDATION_ERROR", "ID tamu wajib diisi.");
    for body in [
        json!({"guest_name": null}),
        json!({"guest_type": null}),
        json!({"guest_name": ""}),
    ] {
        let r = app.call("PUT", &path, Some(&guard), Some(&body)).await;
        assert_eq!(r.0, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    }

    // A guard may only touch their own guests, an admin may touch any.
    let other_guard = t.token(&app, &t.other_guard).await;
    let r = app
        .call(
            "PATCH",
            &path,
            Some(&other_guard),
            Some(&json!({"guest_name": "X"})),
        )
        .await;
    assert_error(
        &r,
        403,
        "AUTH_FORBIDDEN",
        "Petugas (Guard) hanya dapat mengubah data tamu miliknya sendiri.",
    );
    let admin = t.token(&app, &t.admin).await;
    let r = app
        .call(
            "PATCH",
            &path,
            Some(&admin),
            Some(&json!({"guest_name": "Oleh Admin"})),
        )
        .await;
    assert_eq!(r.1["data"]["guest_name"], "Oleh Admin", "{}", r.1);
}

#[tokio::test]
async fn acknowledge_allows_staff_or_the_invited_host_once() {
    let app = TestApp::start().await;
    let t = Tenant::new(&app).await;
    let (id, qr) = t.seed(&app, "Fani", &t.guard, "2026-01-01 08:00:00").await;
    sqlx::query("UPDATE tkguest SET GuestToUserEmail = ? WHERE GuestId = ?")
        .bind(&t.host)
        .bind(id)
        .execute(&app.state.db)
        .await
        .expect("host");
    let stranger = format!("stranger-{}@t.id", Uuid::new_v4().simple());
    let stranger_token = token_with(&app, &stranger, Some(&t.group), Some("USER")).await;
    let path = format!("/api/v1/guests/{id}/acknowledge");

    let r = app.call("PATCH", &path, None, Some(&json!({}))).await;
    assert_error(
        &r,
        401,
        "AUTH_UNAUTHORIZED",
        "Akses tidak diizinkan. Token tidak valid atau telah kedaluwarsa.",
    );
    let r = app
        .call("PATCH", &path, Some(&stranger_token), Some(&json!({})))
        .await;
    assert_error(
        &r,
        403,
        "AUTH_FORBIDDEN",
        "Hanya Admin, Petugas (Guard), atau Penghuni yang dituju yang dapat mengonfirmasi tamu.",
    );
    let r = app
        .call(
            "PATCH",
            "/api/v1/guests/987654321/acknowledge",
            Some(&stranger_token),
            Some(&json!({})),
        )
        .await;
    assert_error(&r, 404, "RESOURCE_NOT_FOUND", "Data tamu tidak ditemukan.");
    let r = app
        .call(
            "PATCH",
            "/api/v1/guests/null/acknowledge",
            Some(&stranger_token),
            Some(&json!({})),
        )
        .await;
    assert_error(&r, 400, "VALIDATION_ERROR", "ID atau QR tamu wajib diisi.");
    let r = app.call("PATCH", &path, Some(&stranger_token), None).await;
    assert_eq!(r.0, StatusCode::UNPROCESSABLE_ENTITY, "a body is required");

    // The QR code in the body wins over the path.
    let host_token = t.token(&app, &t.host).await;
    let (status, body) = app
        .call(
            "PATCH",
            "/api/v1/guests/987654321/acknowledge",
            Some(&host_token),
            Some(&json!({"qr_uid": qr})),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["message"], "Data tamu berhasil dikonfirmasi");
    let acked = body["data"]["guest_ack_dt"].as_str().expect("ack time");
    assert!(acked.ends_with('Z'));

    let r = app
        .call(
            "PATCH",
            &path,
            Some(&host_token),
            Some(&json!({"qr_uid": null})),
        )
        .await;
    assert_error(
        &r,
        400,
        "VALIDATION_ERROR",
        "Kunjungan tamu sudah dikonfirmasi sebelumnya.",
    );
}

#[tokio::test]
async fn checkout_is_staff_only_and_final() {
    let app = TestApp::start().await;
    let t = Tenant::new(&app).await;
    let (id, qr) = t.seed(&app, "Gita", &t.guard, "2026-01-01 08:00:00").await;
    let (id2, _) = t.seed(&app, "Hadi", &t.guard, "2026-01-01 09:00:00").await;
    let path = format!("/api/v1/guests/{id}/checkout");
    let other_guard = t.token(&app, &t.other_guard).await;

    let (status, body) = app
        .call("PATCH", &path, Some(&other_guard), Some(&json!({})))
        .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "checkout is not limited to the creator: {body}"
    );
    assert_eq!(body["message"], "Tamu berhasil check-out");
    assert_eq!(
        body["data"]["guest_checkout_by_email"],
        t.other_guard.as_str()
    );
    assert_eq!(body["data"]["guest_checkout_by_name"], "Satpam Dua");
    assert!(
        body["data"]["guest_out_dt"]
            .as_str()
            .is_some_and(|d| d.ends_with('Z'))
    );

    let admin = t.token(&app, &t.admin).await;
    let r = app
        .call("PATCH", &path, Some(&admin), Some(&json!({})))
        .await;
    assert_error(
        &r,
        400,
        "VALIDATION_ERROR",
        "Tamu sudah keluar/selesai kunjungan sebelumnya.",
    );
    let r = app
        .call(
            "PATCH",
            "/api/v1/guests/x/checkout",
            Some(&admin),
            Some(&json!({"qr_uid": qr})),
        )
        .await;
    assert_error(
        &r,
        400,
        "VALIDATION_ERROR",
        "Tamu sudah keluar/selesai kunjungan sebelumnya.",
    );
    let r = app
        .call(
            "PATCH",
            "/api/v1/guests/987654321/checkout",
            Some(&admin),
            Some(&json!({})),
        )
        .await;
    assert_error(&r, 404, "RESOURCE_NOT_FOUND", "Data tamu tidak ditemukan.");

    // Acknowledging after checkout is refused.
    let r = app
        .call(
            "PATCH",
            &format!("/api/v1/guests/{id}/acknowledge"),
            Some(&admin),
            Some(&json!({})),
        )
        .await;
    assert_error(
        &r,
        400,
        "VALIDATION_ERROR",
        "Tamu sudah keluar/selesai kunjungan, tidak dapat dikonfirmasi lagi.",
    );

    let (status, body) = app
        .call(
            "PATCH",
            &format!("/api/v1/guests/{id2}/checkout"),
            Some(&admin),
            Some(&json!({"qr_uid": null})),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
}

#[tokio::test]
async fn delete_single_hard_deletes_with_ownership_rules() {
    let app = TestApp::start().await;
    let t = Tenant::new(&app).await;
    let other = Tenant::new(&app).await;
    let (own, own_qr) = t
        .seed(&app, "Milik Sendiri", &t.guard, "2026-01-01 08:00:00")
        .await;
    let (theirs, _) = t
        .seed(&app, "Milik Lain", &t.other_guard, "2026-01-01 09:00:00")
        .await;
    let (foreign, _) = other
        .seed(&app, "Asing", &other.guard, "2026-01-01 10:00:00")
        .await;
    let guard = t.token(&app, &t.guard).await;
    let delete = |id: String, token: &str| {
        let app = &app;
        let token = token.to_owned();
        async move {
            app.call(
                "DELETE",
                &format!("/api/v1/guests/{id}"),
                Some(&token),
                None,
            )
            .await
        }
    };

    let r = delete(theirs.to_string(), &guard).await;
    assert_error(
        &r,
        403,
        "AUTH_FORBIDDEN",
        "Petugas (Guard) hanya dapat menghapus data tamu miliknya sendiri.",
    );
    let r = delete(foreign.to_string(), &guard).await;
    assert_error(&r, 404, "RESOURCE_NOT_FOUND", "Data tamu tidak ditemukan.");
    let r = delete("0".into(), &guard).await;
    assert_error(&r, 404, "RESOURCE_NOT_FOUND", "Data tamu tidak ditemukan.");
    let r = delete(own_qr, &guard).await;
    assert_error(&r, 404, "RESOURCE_NOT_FOUND", "Data tamu tidak ditemukan.");
    let r = delete("null".into(), &guard).await;
    assert_error(&r, 400, "VALIDATION_ERROR", "ID tamu wajib diisi.");

    let (status, body) = delete(own.to_string(), &guard).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["message"], "Data tamu berhasil dihapus");
    assert_eq!(body["data"], Value::Null);
    let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM tkguest WHERE GuestId = ?")
        .bind(own)
        .fetch_one(&app.state.db)
        .await
        .expect("count");
    assert_eq!(count, 0, "hard delete");
    let r = delete(own.to_string(), &guard).await;
    assert_eq!(r.0, StatusCode::NOT_FOUND);

    let admin = t.token(&app, &t.admin).await;
    let (status, _) = delete(theirs.to_string(), &admin).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "admins may delete any guest of the group"
    );
}

#[tokio::test]
async fn delete_batch_reports_the_count_and_validates_the_list() {
    let app = TestApp::start().await;
    let t = Tenant::new(&app).await;
    let (a, _) = t.seed(&app, "A", &t.guard, "2026-01-01 08:00:00").await;
    let (b, _) = t.seed(&app, "B", &t.guard, "2026-01-01 09:00:00").await;
    let (c, _) = t
        .seed(&app, "C", &t.other_guard, "2026-01-01 10:00:00")
        .await;
    let guard = t.token(&app, &t.guard).await;
    let batch = |ids: Value, token: &str| {
        let app = &app;
        let token = token.to_owned();
        async move {
            app.call(
                "DELETE",
                "/api/v1/guests",
                Some(&token),
                Some(&json!({ "ids": ids })),
            )
            .await
        }
    };

    for (body, detail) in [
        (
            json!({"ids": []}),
            "/ids: Expected array length to be greater or equal to 1",
        ),
        (
            json!({"ids": ["1", "abc"]}),
            "/ids/1: Expected string to match '^[0-9]+$'",
        ),
        (
            json!({"ids": ["-1"]}),
            "/ids/0: Expected string to match '^[0-9]+$'",
        ),
        (
            json!({"ids": [1]}),
            "/ids/0: invalid type: integer `1`, expected a string",
        ),
        (json!({}), "/ids: Expected required property"),
    ] {
        let r = app
            .call("DELETE", "/api/v1/guests", Some(&guard), Some(&body))
            .await;
        assert_eq!(r.0, StatusCode::UNPROCESSABLE_ENTITY, "{body}: {}", r.1);
        assert_details(&r.1, detail);
    }
    let r = app
        .call(
            "DELETE",
            "/api/v1/guests",
            None,
            Some(&json!({"ids": ["1"]})),
        )
        .await;
    assert_error(
        &r,
        401,
        "AUTH_UNAUTHORIZED",
        "Akses tidak diizinkan. Token tidak valid atau telah kedaluwarsa.",
    );

    let r = batch(json!([a.to_string(), "987654321"]), &guard).await;
    assert_error(
        &r,
        404,
        "RESOURCE_NOT_FOUND",
        "Satu atau lebih data tamu tidak ditemukan dalam grup aktif.",
    );
    let r = batch(json!(["987654321"]), &guard).await;
    assert_error(&r, 404, "RESOURCE_NOT_FOUND", "Data tamu tidak ditemukan.");
    let r = batch(json!(["0"]), &guard).await;
    assert_error(&r, 404, "RESOURCE_NOT_FOUND", "Data tamu tidak ditemukan.");
    let r = batch(json!([a.to_string(), c.to_string()]), &guard).await;
    assert_error(
        &r,
        403,
        "AUTH_FORBIDDEN",
        "Petugas (Guard) hanya dapat menghapus data tamu miliknya sendiri.",
    );
    // "01" and "1" are different strings that name one guest, so the lengths differ.
    let r = batch(json!([a.to_string(), format!("0{a}")]), &guard).await;
    assert_error(
        &r,
        404,
        "RESOURCE_NOT_FOUND",
        "Satu atau lebih data tamu tidak ditemukan dalam grup aktif.",
    );

    let (status, body) = batch(json!([a.to_string(), b.to_string(), a.to_string()]), &guard).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["message"], "2 data tamu berhasil dihapus");
    assert_eq!(body["data"], json!({"deleted_count": 2}));

    let admin = t.token(&app, &t.admin).await;
    let (status, body) = batch(json!([c.to_string()]), &admin).await;
    assert_eq!(
        (status, &body["data"]["deleted_count"]),
        (StatusCode::OK, &json!(1))
    );
    assert_eq!(body["message"], "1 data tamu berhasil dihapus");
}
