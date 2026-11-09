//! End-to-end tests of `/api/v1/auth/*`. Expectations come from the TypeScript use cases and the
//! unit tests in `tests/unit/auth` on `legacy/typescript`. They need the scratch services of `scripts/dev-services.sh`.

mod common;

use std::sync::atomic::{AtomicU8, Ordering};

use axum::http::{HeaderMap, StatusCode};
use common::TestApp;
use kawalo_api::core::security::{hash_token, verify_access_token};
use redis::AsyncCommands;
use serde_json::{Value, json};
use uuid::Uuid;

const PASSWORD: &str = "rahasia123";
const INACTIVE_MESSAGE: &str =
    "Akun Anda belum aktif atau belum disetujui. Silakan hubungi administrator Anda.";
const NOT_MEMBER_MESSAGE: &str = "Anda tidak terdaftar sebagai anggota pada grup yang dipilih";
const WRONG_CREDENTIALS: &str = "Email atau password yang Anda masukkan salah";

struct Seeded {
    email: String,
    user_id: u64,
    groups: Vec<String>,
}

/// One membership: `(status, category)`, stored exactly as given.
type Membership = (&'static str, &'static str);

async fn seed(app: &TestApp, memberships: &[Membership]) -> Seeded {
    let unique = Uuid::new_v4().simple().to_string();
    let email = format!("u{unique}@auth.test");
    let user_id = sqlx::query(
        "INSERT INTO tkuser (UserUID, UserEmail, UserName, Password) VALUES (?, ?, ?, ?)",
    )
    .bind(format!("usr-{unique}"))
    .bind(&email)
    .bind("Petugas Uji")
    .bind(PASSWORD)
    .execute(&app.state.db)
    .await
    .expect("insert user")
    .last_insert_id();

    let mut groups = Vec::new();
    for (index, (status, category)) in memberships.iter().enumerate() {
        let group = format!("g-{unique}-{index}");
        sqlx::query(
            "INSERT INTO tkgroup (GroupUID, GroupName, GroupOwner, Status, Features) \
             VALUES (?, ?, 'owner@auth.test', 'ACTIVE', ' PATROL; GUEST;;N/A;')",
        )
        .bind(&group)
        .bind(format!("Grup {unique} {index}"))
        .execute(&app.state.db)
        .await
        .expect("insert group");
        let admin = if category.eq_ignore_ascii_case("admin") {
            "yes"
        } else {
            "NO"
        };
        sqlx::query(
            "INSERT INTO tkusergroup (Email, GroupUID, AllowAdmin, AllowPatrol, Category, \
             AllowMonPatrol, Status) VALUES (?, ?, ?, 'YES', ?, 'NO', ?)",
        )
        .bind(&email)
        .bind(&group)
        .bind(admin)
        .bind(category)
        .bind(status)
        .execute(&app.state.db)
        .await
        .expect("insert membership");
        groups.push(group);
    }
    Seeded {
        email,
        user_id,
        groups,
    }
}

/// A distinct client IP per test so the per-IP login counters never overlap.
fn unique_peer() -> String {
    static NEXT: AtomicU8 = AtomicU8::new(1);
    let random = Uuid::new_v4();
    let bytes = random.as_bytes();
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    format!("100.{}.{}.{n}:50000", bytes[0], bytes[1])
}

struct Client<'a> {
    app: &'a TestApp,
    peer: String,
}

impl<'a> Client<'a> {
    fn new(app: &'a TestApp) -> Self {
        Self {
            app,
            peer: unique_peer(),
        }
    }

    async fn post(&self, path: &str, token: Option<&str>, body: &Value) -> (StatusCode, Value) {
        let (status, _, json) = self.post_full(path, token, Some(body)).await;
        (status, json)
    }

    async fn post_full(
        &self,
        path: &str,
        token: Option<&str>,
        body: Option<&Value>,
    ) -> (StatusCode, HeaderMap, Value) {
        self.app
            .call_full(
                "POST",
                &format!("/api/v1/auth{path}"),
                token,
                body,
                &self.peer,
            )
            .await
    }

    async fn login(&self, email: &str, password: &str, group: Option<&str>) -> (StatusCode, Value) {
        let mut body = json!({"email": email, "password": password});
        if let Some(group) = group {
            body["group_uid"] = json!(group);
        }
        self.post("/login", None, &body).await
    }

    /// Logs in successfully and returns `(access_token, refresh_token, data)`.
    async fn login_ok(&self, email: &str, group: Option<&str>) -> (String, String, Value) {
        let (status, body) = self.login(email, PASSWORD, group).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let tokens = &body["data"]["tokens"];
        (
            tokens["access_token"].as_str().expect("access").to_owned(),
            tokens["refresh_token"]
                .as_str()
                .expect("refresh")
                .to_owned(),
            body["data"].clone(),
        )
    }

    async fn refresh(&self, refresh_token: &str) -> (StatusCode, Value) {
        self.post("/refresh", None, &json!({"refresh_token": refresh_token}))
            .await
    }
}

fn assert_error(
    (status, body): &(StatusCode, Value),
    want_status: StatusCode,
    code: &str,
    message: &str,
) {
    assert_eq!(*status, want_status, "{body}");
    assert_eq!(body["success"], false, "{body}");
    assert_eq!(body["error_code"], code, "{body}");
    assert_eq!(body["message"], message, "{body}");
}

async fn redis_conn(app: &TestApp) -> redis::aio::ConnectionManager {
    app.state.redis.clone()
}

fn claims(app: &TestApp, access_token: &str) -> kawalo_api::core::security::JwtUserPayload {
    verify_access_token(&app.state.keys, access_token).expect("token verifies")
}

mod login {
    use super::*;

    #[tokio::test]
    async fn issues_tokens_and_the_user_with_the_active_group() {
        let app = TestApp::start().await;
        let client = Client::new(&app);
        let seeded = seed(&app, &[("JOIN_WAIT", "USER"), ("JOINED", "admin")]).await;

        let (status, body) = client.login(&seeded.email, PASSWORD, None).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["success"], true);
        assert_eq!(body["message"], "Login berhasil");

        let data = &body["data"];
        assert_eq!(data["user"]["id"], seeded.user_id.to_string());
        assert_eq!(data["user"]["email"], seeded.email);
        assert_eq!(data["user"]["name"], "Petugas Uji");
        assert_eq!(data["user"]["avatar_url"], Value::Null);
        assert!(data["user"].get("password").is_none());

        // The first JOINED membership is active, not the first membership.
        let active = &data["user"]["active_group"];
        assert_eq!(active["group_uid"], seeded.groups[1]);
        assert_eq!(active["category"], "ADMIN");
        assert_eq!(active["status"], "JOINED");
        assert_eq!(active["group_status"], "ACTIVE");
        assert_eq!(active["group_owner"], "owner@auth.test");
        assert_eq!(active["group_logo"], Value::Null);
        assert_eq!(active["allow_admin"], true);
        assert_eq!(active["allow_patrol"], true);
        assert_eq!(active["allow_mon_patrol"], false);
        assert_eq!(active["features"], json!(["PATROL", "GUEST"]));
        let groups = data["user"]["groups"].as_array().expect("groups");
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0]["group_uid"], seeded.groups[0]);
        assert_eq!(groups[0]["status"], "JOIN_WAIT");

        assert_eq!(
            active
                .as_object()
                .expect("object")
                .keys()
                .collect::<Vec<_>>(),
            [
                "group_uid",
                "group_name",
                "group_owner",
                "group_logo",
                "group_status",
                "category",
                "status",
                "allow_admin",
                "allow_patrol",
                "allow_mon_patrol",
                "features"
            ]
        );
        assert_eq!(
            data["tokens"]
                .as_object()
                .expect("object")
                .keys()
                .collect::<Vec<_>>(),
            ["access_token", "refresh_token", "token_type", "expires_in"]
        );
        assert_eq!(data["tokens"]["token_type"], "Bearer");
        assert_eq!(
            data["tokens"]["expires_in"],
            app.state.config.jwt_expires_in
        );

        let token = data["tokens"]["access_token"].as_str().expect("access");
        let user = claims(&app, token);
        assert_eq!(
            user.sub.as_deref(),
            Some(seeded.user_id.to_string().as_str())
        );
        assert_eq!(user.email.as_deref(), Some(seeded.email.as_str()));
        assert_eq!(user.role.as_deref(), Some("ADMIN"));
        assert_eq!(user.group_uid.as_deref(), Some(seeded.groups[1].as_str()));
        assert!(user.allow_admin && user.allow_patrol && !user.allow_mon_patrol);
        assert_eq!(user.session_ver, 1);
    }

    #[tokio::test]
    async fn stores_a_hashed_refresh_session_in_redis() {
        let app = TestApp::start().await;
        let client = Client::new(&app);
        let seeded = seed(&app, &[("JOINED", "GUARD")]).await;
        let (_, refresh_token, _) = client.login_ok(&seeded.email, None).await;

        assert_eq!(refresh_token.len(), 64);
        assert!(refresh_token.bytes().all(|b| b.is_ascii_hexdigit()));
        let hash = hash_token(&refresh_token);
        let mut redis = redis_conn(&app).await;
        let raw: String = redis
            .get(format!("auth:refresh:{hash}"))
            .await
            .expect("session");
        let session: Value = serde_json::from_str(&raw).expect("json");
        assert_eq!(
            session
                .as_object()
                .expect("object")
                .keys()
                .collect::<Vec<_>>(),
            [
                "user_id",
                "family_id",
                "active_group_uid",
                "created_at",
                "expires_at"
            ]
        );
        assert_eq!(session["user_id"], seeded.user_id.to_string());
        assert_eq!(session["active_group_uid"], seeded.groups[0]);
        let lifetime = session["expires_at"].as_i64().expect("exp")
            - session["created_at"].as_i64().expect("iat");
        assert_eq!(lifetime, 1_209_600);
        let ttl: i64 = redis
            .ttl(format!("auth:refresh:{hash}"))
            .await
            .expect("ttl");
        assert!((1_209_000..=1_209_600).contains(&ttl), "ttl {ttl}");
        let in_user_index: Option<f64> = redis
            .zscore(format!("auth:user:{}:sessions", seeded.user_id), &hash)
            .await
            .expect("zscore");
        assert_eq!(in_user_index, session["expires_at"].as_f64());
        let family = session["family_id"].as_str().expect("family");
        let in_family: Option<f64> = redis
            .zscore(format!("auth:family:{family}:sessions"), &hash)
            .await
            .expect("zscore");
        assert!(in_family.is_some());
    }

    #[tokio::test]
    async fn email_is_case_insensitive_and_password_is_trimmed() {
        let app = TestApp::start().await;
        let client = Client::new(&app);
        let seeded = seed(&app, &[("JOINED", "USER")]).await;

        let upper = seeded.email.to_uppercase();
        let (status, body) = client.login(&upper, &format!("  {PASSWORD}  "), None).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["data"]["user"]["email"], seeded.email);
    }

    #[tokio::test]
    async fn requested_group_becomes_the_active_group() {
        let app = TestApp::start().await;
        let client = Client::new(&app);
        let seeded = seed(&app, &[("JOINED", "ADMIN"), ("JOINED", "GUARD")]).await;

        let (access, _, data) = client
            .login_ok(&seeded.email, Some(&seeded.groups[1]))
            .await;
        assert_eq!(data["user"]["active_group"]["group_uid"], seeded.groups[1]);
        let user = claims(&app, &access);
        assert_eq!(user.group_uid.as_deref(), Some(seeded.groups[1].as_str()));
        assert_eq!(user.role.as_deref(), Some("GUARD"));
        assert!(!user.allow_admin);
    }

    #[tokio::test]
    async fn blank_group_uid_is_ignored() {
        let app = TestApp::start().await;
        let client = Client::new(&app);
        let seeded = seed(&app, &[("JOINED", "GUARD")]).await;
        let (_, _, data) = client.login_ok(&seeded.email, Some("  ")).await;
        assert_eq!(data["user"]["active_group"]["group_uid"], seeded.groups[0]);
    }

    #[tokio::test]
    async fn wrong_password_and_unknown_user_look_the_same() {
        let app = TestApp::start().await;
        let client = Client::new(&app);
        let seeded = seed(&app, &[("JOINED", "USER")]).await;

        let wrong = client.login(&seeded.email, "salah", None).await;
        assert_error(
            &wrong,
            StatusCode::UNAUTHORIZED,
            "AUTH_INVALID_CREDENTIALS",
            WRONG_CREDENTIALS,
        );
        let unknown = client.login("nobody-here@auth.test", PASSWORD, None).await;
        assert_error(
            &unknown,
            StatusCode::UNAUTHORIZED,
            "AUTH_INVALID_CREDENTIALS",
            WRONG_CREDENTIALS,
        );
        // Passwords are compared case-sensitively.
        let cased = client
            .login(&seeded.email, &PASSWORD.to_uppercase(), None)
            .await;
        assert_eq!(cased.0, StatusCode::UNAUTHORIZED);
        // A placeholder password cleans to nothing.
        let placeholder = client.login(&seeded.email, "N/A", None).await;
        assert_error(
            &placeholder,
            StatusCode::UNAUTHORIZED,
            "AUTH_INVALID_CREDENTIALS",
            WRONG_CREDENTIALS,
        );
    }

    #[tokio::test]
    async fn requested_group_without_membership_is_forbidden() {
        let app = TestApp::start().await;
        let client = Client::new(&app);
        let seeded = seed(&app, &[("JOINED", "USER")]).await;

        let result = client
            .login(&seeded.email, PASSWORD, Some("grup-asing"))
            .await;
        assert_error(
            &result,
            StatusCode::FORBIDDEN,
            "AUTH_FORBIDDEN",
            NOT_MEMBER_MESSAGE,
        );
        // The password was right, so the failure counter is cleared, not incremented.
        let mut redis = redis_conn(&app).await;
        let fails: Option<String> = redis
            .get(format!("rl:login:email:fails:{}", seeded.email))
            .await
            .expect("get");
        assert_eq!(fails, None);
    }

    #[tokio::test]
    async fn memberships_that_are_not_joined_cannot_log_in() {
        let app = TestApp::start().await;
        let client = Client::new(&app);
        let seeded = seed(&app, &[("JOIN_WAIT", "USER"), ("REJECTED", "USER")]).await;

        let none_joined = client.login(&seeded.email, PASSWORD, None).await;
        assert_error(
            &none_joined,
            StatusCode::FORBIDDEN,
            "AUTH_USER_INACTIVE",
            INACTIVE_MESSAGE,
        );
        let pending = client
            .login(&seeded.email, PASSWORD, Some(&seeded.groups[0]))
            .await;
        assert_error(
            &pending,
            StatusCode::FORBIDDEN,
            "AUTH_USER_INACTIVE",
            INACTIVE_MESSAGE,
        );

        let no_groups = seed(&app, &[]).await;
        let result = client.login(&no_groups.email, PASSWORD, None).await;
        assert_error(
            &result,
            StatusCode::FORBIDDEN,
            "AUTH_USER_INACTIVE",
            INACTIVE_MESSAGE,
        );
    }

    #[tokio::test]
    async fn invalid_bodies_are_422_before_anything_else() {
        let app = TestApp::start().await;
        let client = Client::new(&app);
        for body in [
            json!({"email": "bukan-email", "password": "x"}),
            json!({"email": "a@b.co", "password": ""}),
            json!({"email": "a@b.co"}),
            json!({"password": "x"}),
            json!({"email": "a@b.co", "password": "x", "group_uid": null}),
            json!({"email": 1, "password": "x"}),
            json!([]),
        ] {
            let (status, response) = client.post("/login", None, &body).await;
            assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
            assert_eq!(response["error_code"], "VALIDATION_ERROR");
            assert_eq!(response["message"], "Format data masukan tidak valid");
        }
        let (status, _, response) = client.post_full("/login", None, None).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{response}");
    }

    #[tokio::test]
    async fn five_failures_lock_the_account_for_fifteen_minutes() {
        let app = TestApp::start().await;
        let client = Client::new(&app);
        let seeded = seed(&app, &[("JOINED", "USER")]).await;

        for attempt in 1..=5 {
            let result = client.login(&seeded.email, "salah", None).await;
            assert_eq!(result.0, StatusCode::UNAUTHORIZED, "attempt {attempt}");
        }
        let mut redis = redis_conn(&app).await;
        let fails_key = format!("rl:login:email:fails:{}", seeded.email);
        let lock_key = format!("rl:login:email:locked:{}", seeded.email);
        let fails: Option<String> = redis.get(&fails_key).await.expect("fails");
        assert_eq!(fails, None, "the counter is cleared once the lock is set");
        let lock: Option<String> = redis.get(&lock_key).await.expect("lock");
        assert_eq!(lock.as_deref(), Some("1"));

        // Even the right password is refused while locked.
        let (status, headers, body) = client
            .post_full(
                "/login",
                None,
                Some(&json!({"email": seeded.email, "password": PASSWORD})),
            )
            .await;
        assert_eq!(status, StatusCode::TOO_MANY_REQUESTS, "{body}");
        assert_eq!(body["error_code"], "AUTH_ACCOUNT_LOCKED");
        assert_eq!(
            body["message"],
            "Akun Anda dikunci sementara karena beberapa kali kegagalan masuk. Silakan coba kembali dalam 15 menit."
        );
        let retry: u64 = headers["retry-after"]
            .to_str()
            .expect("str")
            .parse()
            .expect("number");
        assert!((895..=900).contains(&retry), "retry-after {retry}");

        // Expiring the lock lets the user in and a success wipes the failure history.
        let _: () = redis.del(&lock_key).await.expect("unlock");
        let (_, _, _) = client.login_ok(&seeded.email, None).await;
    }

    #[tokio::test]
    async fn success_resets_the_failure_counter() {
        let app = TestApp::start().await;
        let client = Client::new(&app);
        let seeded = seed(&app, &[("JOINED", "USER")]).await;

        for _ in 0..4 {
            client.login(&seeded.email, "salah", None).await;
        }
        client.login_ok(&seeded.email, None).await;
        let mut redis = redis_conn(&app).await;
        let fails: Option<String> = redis
            .get(format!("rl:login:email:fails:{}", seeded.email))
            .await
            .expect("fails");
        assert_eq!(fails, None);
        // Four more failures are below the limit again.
        for _ in 0..4 {
            client.login(&seeded.email, "salah", None).await;
        }
        client.login_ok(&seeded.email, None).await;
    }

    #[tokio::test]
    async fn unknown_emails_count_towards_the_ip_and_their_own_lock() {
        let app = TestApp::start().await;
        let client = Client::new(&app);
        let email = format!("ghost{}@auth.test", Uuid::new_v4().simple());
        for _ in 0..5 {
            client.login(&email, "x", None).await;
        }
        let (status, body) = client.login(&email, "x", None).await;
        assert_eq!(status, StatusCode::TOO_MANY_REQUESTS, "{body}");
        assert_eq!(body["error_code"], "AUTH_ACCOUNT_LOCKED");
    }

    #[tokio::test]
    async fn ip_limit_blocks_every_account_from_that_address() {
        let app = TestApp::start_with(|config| config.auth.login_rate_limit_ip_max = 3).await;
        let client = Client::new(&app);
        let seeded = seed(&app, &[("JOINED", "USER")]).await;
        for i in 0..3 {
            let email = format!("ghost{i}{}@auth.test", Uuid::new_v4().simple());
            assert_eq!(
                client.login(&email, "x", None).await.0,
                StatusCode::UNAUTHORIZED
            );
        }

        let (status, headers, body) = client
            .post_full(
                "/login",
                None,
                Some(&json!({"email": seeded.email, "password": PASSWORD})),
            )
            .await;
        assert_eq!(status, StatusCode::TOO_MANY_REQUESTS, "{body}");
        assert_eq!(body["error_code"], "RATE_LIMIT_EXCEEDED");
        assert_eq!(
            body["message"],
            "Terlalu banyak permintaan login dari IP ini. Silakan coba kembali dalam 15 menit."
        );
        let retry: u64 = headers["retry-after"]
            .to_str()
            .expect("str")
            .parse()
            .expect("number");
        assert!((895..=900).contains(&retry), "retry-after {retry}");

        // Another address is unaffected.
        let other = Client::new(&app);
        other.login_ok(&seeded.email, None).await;
    }

    #[tokio::test]
    async fn ip_counter_expires_after_the_window() {
        let app = TestApp::start().await;
        let client = Client::new(&app);
        let ip = client.peer.split(':').next().expect("ip").to_owned();
        let email = format!("ghost{}@auth.test", Uuid::new_v4().simple());
        client.login(&email, "x", None).await;
        let mut redis = redis_conn(&app).await;
        let ttl: i64 = redis.ttl(format!("rl:login:ip:{ip}")).await.expect("ttl");
        assert!((895..=900).contains(&ttl), "ttl {ttl}");
        let fail_ttl: i64 = redis
            .ttl(format!("rl:login:email:fails:{email}"))
            .await
            .expect("ttl");
        assert!((895..=900).contains(&fail_ttl), "ttl {fail_ttl}");
    }
}

mod refresh {
    use super::*;

    #[tokio::test]
    async fn rotates_the_token_inside_the_same_family() {
        let app = TestApp::start().await;
        let client = Client::new(&app);
        let seeded = seed(&app, &[("JOINED", "GUARD")]).await;
        let (_, old_refresh, _) = client.login_ok(&seeded.email, None).await;

        let (status, body) = client.refresh(&old_refresh).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["message"], "Token berhasil diperbarui");
        let tokens = &body["data"];
        assert_eq!(
            tokens
                .as_object()
                .expect("object")
                .keys()
                .collect::<Vec<_>>(),
            ["access_token", "refresh_token", "token_type", "expires_in"]
        );
        let new_refresh = tokens["refresh_token"].as_str().expect("refresh");
        assert_ne!(new_refresh, old_refresh);
        let user = claims(&app, tokens["access_token"].as_str().expect("access"));
        assert_eq!(user.group_uid.as_deref(), Some(seeded.groups[0].as_str()));
        assert_eq!(
            user.session_ver, 1,
            "refreshing does not bump the session version"
        );

        let mut redis = redis_conn(&app).await;
        let old_hash = hash_token(&old_refresh);
        let new_hash = hash_token(new_refresh);
        let old_session: Option<String> = redis
            .get(format!("auth:refresh:{old_hash}"))
            .await
            .expect("old");
        assert_eq!(old_session, None);
        let new_session: String = redis
            .get(format!("auth:refresh:{new_hash}"))
            .await
            .expect("new");
        let family: Option<String> = redis
            .get(format!("auth:tombstone:{old_hash}"))
            .await
            .expect("tombstone");
        assert_eq!(
            family.as_deref(),
            serde_json::from_str::<Value>(&new_session).expect("json")["family_id"].as_str()
        );
        let user_index: Vec<String> = redis
            .zrange(format!("auth:user:{}:sessions", seeded.user_id), 0, -1)
            .await
            .expect("zrange");
        assert_eq!(user_index, [new_hash]);
        let grace_ttl: i64 = redis
            .ttl(format!("auth:grace:{old_hash}"))
            .await
            .expect("ttl");
        assert!((1..=15).contains(&grace_ttl), "grace ttl {grace_ttl}");
    }

    #[tokio::test]
    async fn replaying_the_old_token_within_the_grace_window_returns_the_same_tokens() {
        let app = TestApp::start().await;
        let client = Client::new(&app);
        let seeded = seed(&app, &[("JOINED", "GUARD")]).await;
        let (_, old_refresh, _) = client.login_ok(&seeded.email, None).await;

        let first = client.refresh(&old_refresh).await;
        let replay = client.refresh(&old_refresh).await;
        assert_eq!(first.0, StatusCode::OK);
        assert_eq!(replay, first);
    }

    #[tokio::test]
    async fn reuse_after_the_grace_window_revokes_the_whole_family() {
        let app = TestApp::start().await;
        let client = Client::new(&app);
        let seeded = seed(&app, &[("JOINED", "GUARD")]).await;
        let (_, old_refresh, _) = client.login_ok(&seeded.email, None).await;
        let (_, first) = client.refresh(&old_refresh).await;
        let newest = first["data"]["refresh_token"]
            .as_str()
            .expect("refresh")
            .to_owned();

        let mut redis = redis_conn(&app).await;
        let _: () = redis
            .del(format!("auth:grace:{}", hash_token(&old_refresh)))
            .await
            .expect("expire grace");

        let reuse = client.refresh(&old_refresh).await;
        assert_error(
            &reuse,
            StatusCode::UNAUTHORIZED,
            "AUTH_REUSE_DETECTED",
            "Sesi Anda dihentikan demi keamanan karena terdeteksi anomali token. Silakan login kembali.",
        );
        // The legitimate holder of the newest token is logged out too.
        let after = client.refresh(&newest).await;
        assert_error(
            &after,
            StatusCode::UNAUTHORIZED,
            "AUTH_TOKEN_INVALID",
            "Sesi tidak ditemukan atau telah kedaluwarsa",
        );
    }

    #[tokio::test]
    async fn unknown_token_is_401() {
        let app = TestApp::start().await;
        let client = Client::new(&app);
        let result = client.refresh(&"ab".repeat(32)).await;
        assert_error(
            &result,
            StatusCode::UNAUTHORIZED,
            "AUTH_TOKEN_INVALID",
            "Sesi tidak ditemukan atau telah kedaluwarsa",
        );
    }

    #[tokio::test]
    async fn blank_token_is_400_but_empty_is_422() {
        let app = TestApp::start().await;
        let client = Client::new(&app);
        for blank in ["   ", "N/A", "null"] {
            let result = client.refresh(blank).await;
            assert_error(
                &result,
                StatusCode::BAD_REQUEST,
                "AUTH_TOKEN_INVALID",
                "Refresh token wajib disertakan",
            );
        }
        let (status, body) = client.refresh("").await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
        let (status, body) = client.post("/refresh", None, &json!({})).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    }

    #[tokio::test]
    async fn a_token_being_rotated_elsewhere_is_429() {
        let app = TestApp::start().await;
        let client = Client::new(&app);
        let seeded = seed(&app, &[("JOINED", "GUARD")]).await;
        let (_, refresh, _) = client.login_ok(&seeded.email, None).await;
        let hash = hash_token(&refresh);
        let mut redis = redis_conn(&app).await;
        let _: () = redis
            .set_ex(format!("lock:{hash}"), "other", 5)
            .await
            .expect("lock");

        let result = client.refresh(&refresh).await;
        assert_error(
            &result,
            StatusCode::TOO_MANY_REQUESTS,
            "RATE_LIMIT_EXCEEDED",
            "Permintaan pembaruan sesi sedang diproses. Silakan coba beberapa saat lagi.",
        );
        // Another holder's lock is left alone, and the token still works once it is gone.
        let held: Option<String> = redis.get(format!("lock:{hash}")).await.expect("get");
        assert_eq!(held.as_deref(), Some("other"));
        let _: () = redis.del(format!("lock:{hash}")).await.expect("unlock");
        assert_eq!(client.refresh(&refresh).await.0, StatusCode::OK);
        let leftover: Option<String> = redis.get(format!("lock:{hash}")).await.expect("get");
        assert_eq!(leftover, None, "the lock is released after use");
    }

    #[tokio::test]
    async fn expired_session_is_removed_and_reported() {
        let app = TestApp::start().await;
        let client = Client::new(&app);
        let seeded = seed(&app, &[("JOINED", "GUARD")]).await;
        let raw = "cd".repeat(32);
        let hash = hash_token(&raw);
        let mut redis = redis_conn(&app).await;
        let session = json!({
            "user_id": seeded.user_id.to_string(),
            "family_id": Uuid::new_v4().to_string(),
            "active_group_uid": seeded.groups[0],
            "created_at": 1_000,
            "expires_at": 2_000,
        });
        let _: () = redis
            .set_ex(format!("auth:refresh:{hash}"), session.to_string(), 60)
            .await
            .expect("plant");

        let result = client.refresh(&raw).await;
        assert_error(
            &result,
            StatusCode::UNAUTHORIZED,
            "AUTH_TOKEN_EXPIRED",
            "Sesi telah kedaluwarsa. Silakan login kembali.",
        );
        let gone: Option<String> = redis
            .get(format!("auth:refresh:{hash}"))
            .await
            .expect("get");
        assert_eq!(gone, None);
    }

    #[tokio::test]
    async fn corrupt_sessions_read_as_missing() {
        let app = TestApp::start().await;
        let client = Client::new(&app);
        let mut redis = redis_conn(&app).await;
        for (index, value) in [
            "not json",
            r#"{"user_id":"","family_id":"f","created_at":1,"expires_at":9}"#,
            r#"{"user_id":"1","family_id":"f","created_at":0,"expires_at":9}"#,
        ]
        .into_iter()
        .enumerate()
        {
            let raw = format!("{index:02}").repeat(32);
            let _: () = redis
                .set_ex(format!("auth:refresh:{}", hash_token(&raw)), value, 60)
                .await
                .expect("plant");
            let result = client.refresh(&raw).await;
            assert_eq!(result.1["error_code"], "AUTH_TOKEN_INVALID", "{value}");
        }
    }

    #[tokio::test]
    async fn keeps_the_group_chosen_at_login() {
        let app = TestApp::start().await;
        let client = Client::new(&app);
        let seeded = seed(&app, &[("JOINED", "ADMIN"), ("JOINED", "GUARD")]).await;
        let (_, refresh, _) = client
            .login_ok(&seeded.email, Some(&seeded.groups[1]))
            .await;

        let (_, body) = client.refresh(&refresh).await;
        let user = claims(&app, body["data"]["access_token"].as_str().expect("access"));
        assert_eq!(user.group_uid.as_deref(), Some(seeded.groups[1].as_str()));
        assert_eq!(user.role.as_deref(), Some("GUARD"));
    }

    #[tokio::test]
    async fn picks_up_permission_changes() {
        let app = TestApp::start().await;
        let client = Client::new(&app);
        let seeded = seed(&app, &[("JOINED", "GUARD")]).await;
        let (_, refresh, _) = client.login_ok(&seeded.email, None).await;
        sqlx::query(
            "UPDATE tkusergroup SET Category = 'ADMIN', AllowAdmin = 'YES' WHERE Email = ?",
        )
        .bind(&seeded.email)
        .execute(&app.state.db)
        .await
        .expect("promote");

        let (_, body) = client.refresh(&refresh).await;
        let user = claims(&app, body["data"]["access_token"].as_str().expect("access"));
        assert_eq!(user.role.as_deref(), Some("ADMIN"));
        assert!(user.allow_admin);
    }

    #[tokio::test]
    async fn deactivated_membership_is_403() {
        let app = TestApp::start().await;
        let client = Client::new(&app);
        let seeded = seed(&app, &[("JOINED", "GUARD")]).await;
        let (_, refresh, _) = client.login_ok(&seeded.email, None).await;
        sqlx::query("UPDATE tkusergroup SET Status = 'KICKED' WHERE Email = ?")
            .bind(&seeded.email)
            .execute(&app.state.db)
            .await
            .expect("deactivate");

        let result = client.refresh(&refresh).await;
        assert_error(
            &result,
            StatusCode::FORBIDDEN,
            "AUTH_USER_INACTIVE",
            INACTIVE_MESSAGE,
        );
    }

    #[tokio::test]
    async fn deleted_user_is_401() {
        let app = TestApp::start().await;
        let client = Client::new(&app);
        let seeded = seed(&app, &[("JOINED", "GUARD")]).await;
        let (_, refresh, _) = client.login_ok(&seeded.email, None).await;
        sqlx::query("DELETE FROM tkuser WHERE UserId = ?")
            .bind(seeded.user_id)
            .execute(&app.state.db)
            .await
            .expect("delete");

        let result = client.refresh(&refresh).await;
        assert_error(
            &result,
            StatusCode::UNAUTHORIZED,
            "AUTH_USER_NOT_FOUND",
            "Pengguna tidak ditemukan",
        );
    }
}

mod switch_group {
    use super::*;

    async fn switch(client: &Client<'_>, token: Option<&str>, group: &str) -> (StatusCode, Value) {
        client
            .post("/switch-group", token, &json!({"group_uid": group}))
            .await
    }

    #[tokio::test]
    async fn issues_a_new_family_for_the_target_group() {
        let app = TestApp::start().await;
        let client = Client::new(&app);
        let seeded = seed(&app, &[("JOINED", "ADMIN"), ("JOINED", "GUARD")]).await;
        let (access, old_refresh, _) = client.login_ok(&seeded.email, None).await;

        let (status, body) = switch(&client, Some(&access), &seeded.groups[1]).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["message"], "Grup aktif berhasil diperbarui");
        let data = &body["data"];
        assert_eq!(data["user"]["active_group"]["group_uid"], seeded.groups[1]);
        assert_eq!(data["user"]["groups"].as_array().expect("groups").len(), 2);
        let user = claims(
            &app,
            data["tokens"]["access_token"].as_str().expect("access"),
        );
        assert_eq!(user.group_uid.as_deref(), Some(seeded.groups[1].as_str()));
        assert_eq!(user.role.as_deref(), Some("GUARD"));
        assert_eq!(
            user.session_ver, 1,
            "switching does not bump the session version"
        );

        // The new refresh token belongs to the new group; the old one keeps its own session.
        let (_, refreshed) = client
            .refresh(data["tokens"]["refresh_token"].as_str().expect("refresh"))
            .await;
        let user = claims(
            &app,
            refreshed["data"]["access_token"].as_str().expect("access"),
        );
        assert_eq!(user.group_uid.as_deref(), Some(seeded.groups[1].as_str()));
        assert_eq!(client.refresh(&old_refresh).await.0, StatusCode::OK);
    }

    #[tokio::test]
    async fn requires_a_valid_token_after_the_body() {
        let app = TestApp::start().await;
        let client = Client::new(&app);
        let anonymous = switch(&client, None, "g").await;
        assert_error(
            &anonymous,
            StatusCode::UNAUTHORIZED,
            "AUTH_UNAUTHORIZED",
            "Akses tidak diizinkan. Token tidak valid atau telah kedaluwarsa.",
        );
        let garbage = switch(&client, Some("bukan.jwt.valid"), "g").await;
        assert_eq!(garbage.1["error_code"], "AUTH_UNAUTHORIZED");

        // Elysia validates the body first, so a bad body without a token is 422.
        let (status, body) = client
            .post("/switch-group", None, &json!({"group_uid": ""}))
            .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
        let (status, _) = client.post("/switch-group", None, &json!({})).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn rejects_groups_the_user_cannot_use() {
        let app = TestApp::start().await;
        let client = Client::new(&app);
        let seeded = seed(&app, &[("JOINED", "ADMIN"), ("JOIN_WAIT", "GUARD")]).await;
        let (access, _, _) = client.login_ok(&seeded.email, None).await;

        let foreign = switch(&client, Some(&access), "grup-asing").await;
        assert_error(
            &foreign,
            StatusCode::FORBIDDEN,
            "AUTH_FORBIDDEN",
            NOT_MEMBER_MESSAGE,
        );
        let pending = switch(&client, Some(&access), &seeded.groups[1]).await;
        assert_error(
            &pending,
            StatusCode::FORBIDDEN,
            "AUTH_USER_INACTIVE",
            INACTIVE_MESSAGE,
        );
        let blank = switch(&client, Some(&access), "  ").await;
        assert_error(
            &blank,
            StatusCode::BAD_REQUEST,
            "VALIDATION_ERROR",
            "Group UID wajib disertakan",
        );
    }

    #[tokio::test]
    async fn unknown_user_in_a_valid_token_is_401() {
        let app = TestApp::start().await;
        let client = Client::new(&app);
        let token = app.token_for("tidak-ada@auth.test", "g1", false).await;
        let result = switch(&client, Some(&token), "g1").await;
        assert_error(
            &result,
            StatusCode::UNAUTHORIZED,
            "AUTH_USER_NOT_FOUND",
            "Pengguna tidak ditemukan",
        );
    }
}

mod logout {
    use super::*;

    #[tokio::test]
    async fn revokes_the_refresh_token_and_stales_access_tokens() {
        let app = TestApp::start().await;
        let client = Client::new(&app);
        let seeded = seed(&app, &[("JOINED", "ADMIN"), ("JOINED", "GUARD")]).await;
        let (access, refresh, _) = client.login_ok(&seeded.email, None).await;

        let (status, body) = client
            .post("/logout", Some(&access), &json!({"refresh_token": refresh}))
            .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(
            body,
            json!({"success": true, "message": "Logout berhasil", "data": null})
        );

        let mut redis = redis_conn(&app).await;
        let hash = hash_token(&refresh);
        let session: Option<String> = redis
            .get(format!("auth:refresh:{hash}"))
            .await
            .expect("get");
        assert_eq!(session, None);
        let in_index: Option<f64> = redis
            .zscore(format!("auth:user:{}:sessions", seeded.user_id), &hash)
            .await
            .expect("zscore");
        assert_eq!(in_index, None);
        let version: String = redis
            .get(format!("auth:user:{}:session_ver", seeded.user_id))
            .await
            .expect("version");
        assert_eq!(version, "2");

        let refreshed = client.refresh(&refresh).await;
        assert_eq!(refreshed.1["error_code"], "AUTH_TOKEN_INVALID");
        let stale = client
            .post(
                "/switch-group",
                Some(&access),
                &json!({"group_uid": seeded.groups[1]}),
            )
            .await;
        assert_error(
            &stale,
            StatusCode::UNAUTHORIZED,
            "AUTH_SESSION_STALE",
            "Sesi telah diperbarui di sistem. Silakan perbarui sesi Anda.",
        );
        // A fresh login works and carries the bumped version.
        let (new_access, _, _) = client.login_ok(&seeded.email, None).await;
        assert_eq!(claims(&app, &new_access).session_ver, 2);
    }

    #[tokio::test]
    async fn without_a_token_it_still_succeeds_and_revokes_the_refresh_token() {
        let app = TestApp::start().await;
        let client = Client::new(&app);
        let seeded = seed(&app, &[("JOINED", "GUARD")]).await;
        let (access, refresh, _) = client.login_ok(&seeded.email, None).await;

        let (status, body) = client
            .post("/logout", None, &json!({"refresh_token": refresh}))
            .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(client.refresh(&refresh).await.0, StatusCode::UNAUTHORIZED);
        // No identity, no version bump: the access token keeps working.
        let still = client
            .post(
                "/switch-group",
                Some(&access),
                &json!({"group_uid": seeded.groups[0]}),
            )
            .await;
        assert_eq!(still.0, StatusCode::OK, "{}", still.1);
    }

    #[tokio::test]
    async fn accepts_an_empty_object_and_garbage_tokens() {
        let app = TestApp::start().await;
        let client = Client::new(&app);
        for body in [
            json!({}),
            json!({"refresh_token": "tidak-ada"}),
            json!({"refresh_token": ""}),
            json!({"refresh_token": "N/A"}),
        ] {
            let (status, response) = client.post("/logout", Some("bukan.jwt.valid"), &body).await;
            assert_eq!(status, StatusCode::OK, "{body}: {response}");
            assert_eq!(response["message"], "Logout berhasil");
        }
    }

    #[tokio::test]
    async fn a_missing_or_malformed_body_is_422_or_400() {
        let app = TestApp::start().await;
        let client = Client::new(&app);
        let (status, _, _) = client.post_full("/logout", None, None).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        let (status, _) = client
            .post("/logout", None, &json!({"refresh_token": null}))
            .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        for body in [json!("teks"), json!(null), json!(5)] {
            let (status, _) = client.post("/logout", None, &body).await;
            assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
        }
        // TypeBox accepts any array when all properties are optional.
        for body in [json!([]), json!(["x"]), json!([1])] {
            let (status, _) = client.post("/logout", None, &body).await;
            assert_eq!(status, StatusCode::OK, "{body}");
        }
    }

    #[tokio::test]
    async fn a_stale_token_does_not_bump_the_version_again() {
        let app = TestApp::start().await;
        let client = Client::new(&app);
        let seeded = seed(&app, &[("JOINED", "GUARD")]).await;
        let (access, _, _) = client.login_ok(&seeded.email, None).await;
        client.post("/logout", Some(&access), &json!({})).await;
        client.post("/logout", Some(&access), &json!({})).await;
        let mut redis = redis_conn(&app).await;
        let version: String = redis
            .get(format!("auth:user:{}:session_ver", seeded.user_id))
            .await
            .expect("version");
        assert_eq!(version, "2");
    }
}

mod password_hash {
    use super::*;
    use kawalo_api::core::security::password::verify_password;

    async fn stored_hash(app: &TestApp, user_id: u64) -> Option<String> {
        sqlx::query_scalar("SELECT PasswordHash FROM tkuser WHERE UserId = ?")
            .bind(user_id)
            .fetch_one(&app.state.db)
            .await
            .expect("select PasswordHash")
    }

    async fn stored_password(app: &TestApp, user_id: u64) -> String {
        sqlx::query_scalar("SELECT Password FROM tkuser WHERE UserId = ?")
            .bind(user_id)
            .fetch_one(&app.state.db)
            .await
            .expect("select Password")
    }

    async fn set_column(app: &TestApp, user_id: u64, column: &str, value: Option<&str>) {
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "UPDATE tkuser SET {column} = ? WHERE UserId = ?"
        )))
        .bind(value)
        .bind(user_id)
        .execute(&app.state.db)
        .await
        .expect("update column");
    }

    #[tokio::test]
    async fn first_login_stores_a_hash_and_leaves_the_plaintext_alone() {
        let app = TestApp::start().await;
        let client = Client::new(&app);
        let seeded = seed(&app, &[("JOINED", "USER")]).await;
        assert_eq!(stored_hash(&app, seeded.user_id).await, None);

        // Whitespace around the password is trimmed before both comparing and hashing.
        let (status, body) = client
            .login(&seeded.email, &format!(" {PASSWORD} "), None)
            .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let hash = stored_hash(&app, seeded.user_id)
            .await
            .expect("hash stored");
        assert!(
            hash.starts_with("$argon2id$v=19$m=19456,t=2,p=1$"),
            "{hash}"
        );
        assert!(verify_password(&hash, PASSWORD).expect("valid PHC"));
        assert_eq!(stored_password(&app, seeded.user_id).await, PASSWORD);

        // The second login is verified against the hash and does not rewrite it.
        let (access, refresh, _) = client.login_ok(&seeded.email, None).await;
        assert_eq!(stored_hash(&app, seeded.user_id).await, Some(hash));
        assert_eq!(stored_password(&app, seeded.user_id).await, PASSWORD);
        assert!(!access.is_empty());
        let refreshed = client.refresh(&refresh).await;
        assert_eq!(refreshed.0, StatusCode::OK, "{}", refreshed.1);
    }

    #[tokio::test]
    async fn a_blank_hash_column_counts_as_not_hashed() {
        let app = TestApp::start().await;
        let client = Client::new(&app);
        let seeded = seed(&app, &[("JOINED", "USER")]).await;
        set_column(&app, seeded.user_id, "PasswordHash", Some("")).await;

        client.login_ok(&seeded.email, None).await;
        let hash = stored_hash(&app, seeded.user_id)
            .await
            .expect("hash stored");
        assert!(hash.starts_with("$argon2id$"), "{hash}");
    }

    #[tokio::test]
    async fn a_wrong_password_never_stores_a_hash() {
        let app = TestApp::start().await;
        let client = Client::new(&app);
        let seeded = seed(&app, &[("JOINED", "USER")]).await;

        let wrong = client.login(&seeded.email, "salah", None).await;
        assert_error(
            &wrong,
            StatusCode::UNAUTHORIZED,
            "AUTH_INVALID_CREDENTIALS",
            WRONG_CREDENTIALS,
        );
        assert_eq!(stored_hash(&app, seeded.user_id).await, None);
    }

    #[tokio::test]
    async fn hashed_users_fail_and_lock_exactly_like_plaintext_users() {
        let app = TestApp::start().await;
        let client = Client::new(&app);
        let seeded = seed(&app, &[("JOINED", "USER")]).await;
        client.login_ok(&seeded.email, None).await;

        let wrong = client.login(&seeded.email, "salah", None).await;
        assert_error(
            &wrong,
            StatusCode::UNAUTHORIZED,
            "AUTH_INVALID_CREDENTIALS",
            WRONG_CREDENTIALS,
        );
        let cased = client
            .login(&seeded.email, &PASSWORD.to_uppercase(), None)
            .await;
        assert_eq!(cased.0, StatusCode::UNAUTHORIZED);

        for _ in 0..3 {
            client.login(&seeded.email, "salah", None).await;
        }
        let locked = client.login(&seeded.email, PASSWORD, None).await;
        assert_eq!(locked.0, StatusCode::TOO_MANY_REQUESTS, "{}", locked.1);
        assert_eq!(locked.1["error_code"], "AUTH_ACCOUNT_LOCKED");
        assert_eq!(stored_password(&app, seeded.user_id).await, PASSWORD);
    }

    #[tokio::test]
    async fn a_password_changed_elsewhere_beats_the_stale_hash() {
        let app = TestApp::start().await;
        let client = Client::new(&app);
        let seeded = seed(&app, &[("JOINED", "USER")]).await;
        client.login_ok(&seeded.email, None).await;
        let stale = stored_hash(&app, seeded.user_id).await.expect("hash");

        set_column(&app, seeded.user_id, "Password", Some("sandi-baru")).await;
        // The old password still matches the hash but no longer the plain text.
        let old = client.login(&seeded.email, PASSWORD, None).await;
        assert_error(
            &old,
            StatusCode::UNAUTHORIZED,
            "AUTH_INVALID_CREDENTIALS",
            WRONG_CREDENTIALS,
        );
        assert_eq!(stored_hash(&app, seeded.user_id).await, Some(stale.clone()));

        // The new one is accepted and the hash follows it.
        let (status, body) = client.login(&seeded.email, "sandi-baru", None).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let fresh = stored_hash(&app, seeded.user_id).await.expect("hash");
        assert_ne!(fresh, stale);
        assert!(verify_password(&fresh, "sandi-baru").expect("valid PHC"));
        assert!(!verify_password(&fresh, PASSWORD).expect("valid PHC"));
        assert_eq!(stored_password(&app, seeded.user_id).await, "sandi-baru");
        let again = client.login(&seeded.email, "sandi-baru", None).await;
        assert_eq!(again.0, StatusCode::OK, "{}", again.1);
        assert_eq!(stored_hash(&app, seeded.user_id).await, Some(fresh));
    }

    #[tokio::test]
    async fn a_stale_hash_rejection_counts_towards_the_lockout() {
        let app = TestApp::start().await;
        let client = Client::new(&app);
        let seeded = seed(&app, &[("JOINED", "USER")]).await;
        client.login_ok(&seeded.email, None).await;
        set_column(&app, seeded.user_id, "Password", Some("sandi-baru")).await;

        for _ in 0..5 {
            let old = client.login(&seeded.email, PASSWORD, None).await;
            assert_eq!(old.0, StatusCode::UNAUTHORIZED, "{}", old.1);
        }
        let locked = client.login(&seeded.email, "sandi-baru", None).await;
        assert_eq!(locked.0, StatusCode::TOO_MANY_REQUESTS, "{}", locked.1);
        assert_eq!(locked.1["error_code"], "AUTH_ACCOUNT_LOCKED");
    }

    #[tokio::test]
    async fn with_the_plaintext_emptied_only_the_hash_decides() {
        let app = TestApp::start().await;
        let client = Client::new(&app);
        let seeded = seed(&app, &[("JOINED", "USER")]).await;
        client.login_ok(&seeded.email, None).await;

        set_column(&app, seeded.user_id, "Password", Some("")).await;
        client.login_ok(&seeded.email, None).await;
        let wrong = client.login(&seeded.email, "salah", None).await;
        assert_error(
            &wrong,
            StatusCode::UNAUTHORIZED,
            "AUTH_INVALID_CREDENTIALS",
            WRONG_CREDENTIALS,
        );
        assert_eq!(stored_password(&app, seeded.user_id).await, "");
    }

    #[tokio::test]
    async fn no_plaintext_and_no_hash_means_no_login() {
        let app = TestApp::start().await;
        let client = Client::new(&app);
        let seeded = seed(&app, &[("JOINED", "USER")]).await;
        set_column(&app, seeded.user_id, "Password", Some("")).await;

        let result = client.login(&seeded.email, PASSWORD, None).await;
        assert_error(
            &result,
            StatusCode::UNAUTHORIZED,
            "AUTH_INVALID_CREDENTIALS",
            WRONG_CREDENTIALS,
        );
        assert_eq!(stored_hash(&app, seeded.user_id).await, None);
    }

    #[tokio::test]
    async fn a_hash_that_is_not_a_valid_phc_string_is_replaced_by_the_plaintext_check() {
        let app = TestApp::start().await;
        let client = Client::new(&app);
        let seeded = seed(&app, &[("JOINED", "USER")]).await;
        set_column(&app, seeded.user_id, "PasswordHash", Some("bukan-hash")).await;

        let wrong = client.login(&seeded.email, "salah", None).await;
        assert_error(
            &wrong,
            StatusCode::UNAUTHORIZED,
            "AUTH_INVALID_CREDENTIALS",
            WRONG_CREDENTIALS,
        );
        assert_eq!(
            stored_hash(&app, seeded.user_id).await.as_deref(),
            Some("bukan-hash")
        );

        client.login_ok(&seeded.email, None).await;
        let healed = stored_hash(&app, seeded.user_id).await.expect("hash");
        assert!(verify_password(&healed, PASSWORD).expect("valid PHC"));
    }

    #[tokio::test]
    async fn a_hash_written_with_other_parameters_still_verifies() {
        let app = TestApp::start().await;
        let client = Client::new(&app);
        let seeded = seed(&app, &[("JOINED", "USER")]).await;
        // As an older or newer deployment with cheaper parameters would have written it.
        let params = argon2::Params::new(8, 1, 1, None).expect("params");
        let cheap =
            argon2::Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params);
        let phc = argon2::password_hash::PasswordHasher::hash_password(&cheap, PASSWORD.as_bytes())
            .expect("hash")
            .to_string();
        set_column(&app, seeded.user_id, "PasswordHash", Some(&phc)).await;

        client.login_ok(&seeded.email, None).await;
        assert_eq!(stored_hash(&app, seeded.user_id).await, Some(phc));
    }

    #[tokio::test]
    async fn failing_to_store_the_hash_does_not_fail_the_login() {
        let app = TestApp::start().await;
        let client = Client::new(&app);
        let seeded = seed(&app, &[("JOINED", "USER")]).await;

        let trigger = format!("trg_{}", Uuid::new_v4().simple());
        let create = format!(
            "CREATE TRIGGER {trigger} BEFORE UPDATE ON tkuser FOR EACH ROW \
             IF NEW.UserEmail = '{}' THEN SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'boom'; END IF",
            seeded.email
        );
        sqlx::query(sqlx::AssertSqlSafe(create))
            .execute(&app.state.db)
            .await
            .expect("create trigger");

        let result = client.login(&seeded.email, PASSWORD, None).await;
        let _ = sqlx::query(sqlx::AssertSqlSafe(format!(
            "DROP TRIGGER IF EXISTS {trigger}"
        )))
        .execute(&app.state.db)
        .await;
        assert_eq!(result.0, StatusCode::OK, "{}", result.1);
        assert_eq!(result.1["message"], "Login berhasil");
        assert_eq!(stored_hash(&app, seeded.user_id).await, None);
        assert_eq!(stored_password(&app, seeded.user_id).await, PASSWORD);
    }

    #[tokio::test]
    async fn unknown_users_cost_a_verification_too() {
        let app = TestApp::start().await;
        let client = Client::new(&app);
        let seeded = seed(&app, &[("JOINED", "USER")]).await;
        client.login_ok(&seeded.email, None).await;

        // Averaged over a few attempts so scheduler noise does not decide it; the exact figure
        // depends on the machine, but a skipped Argon2 run would be far below half.
        async fn average_ms(client: &Client<'_>, email: &str) -> f64 {
            let started = std::time::Instant::now();
            for _ in 0..3 {
                client.login(email, "salah", None).await;
            }
            started.elapsed().as_secs_f64() * 1000.0 / 3.0
        }
        let known = average_ms(&client, &seeded.email).await;
        let unknown = average_ms(&client, &format!("x{}@auth.test", Uuid::new_v4().simple())).await;
        assert!(
            unknown > known * 0.5,
            "known {known:.1} ms, unknown {unknown:.1} ms"
        );
    }
}

mod routing {
    use super::*;

    #[tokio::test]
    async fn only_post_is_routed() {
        let app = TestApp::start().await;
        let (status, body) = app.call("GET", "/api/v1/auth/login", None, None).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
        assert_eq!(body["error_code"], "RESOURCE_NOT_FOUND");
    }
}
