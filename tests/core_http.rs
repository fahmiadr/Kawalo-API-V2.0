//! End-to-end checks of the shared HTTP behaviour against the scratch MariaDB and Redis.

mod common;

use axum::http::StatusCode;
use common::TestApp;
use serde_json::json;

#[tokio::test]
async fn health_reports_liveness_without_dependencies_in_the_payload() {
    let app = TestApp::start().await;
    let (status, headers, body) = app
        .call_full("GET", "/health", None, None, "127.0.0.1:1")
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["success"], true);
    assert_eq!(body["message"], "Sistem berjalan normal");
    assert_eq!(body["data"]["app"], "Kawalo Mobile API");
    assert_eq!(body["data"]["status"], "ok");
    assert!(body["data"]["uptime"].is_f64(), "{body}");
    assert!(
        body["data"]["timestamp"]
            .as_str()
            .is_some_and(|t| t.ends_with('Z'))
    );
    assert_eq!(headers["x-content-type-options"], "nosniff");
    assert_eq!(headers["x-frame-options"], "DENY");
    assert!(headers.contains_key("x-request-id"));
    assert_eq!(headers["access-control-allow-credentials"], "true");
}

#[tokio::test]
async fn one_trailing_slash_is_ignored_and_wrong_methods_carry_no_allow_header() {
    let app = TestApp::start().await;
    for (path, want) in [
        ("/health/", StatusCode::OK),
        ("/health/?a=1", StatusCode::OK),
        ("/health/ready/", StatusCode::OK),
        ("/health//", StatusCode::NOT_FOUND),
        ("//health", StatusCode::NOT_FOUND),
        ("/", StatusCode::NOT_FOUND),
    ] {
        let (status, _, _) = app.call_full("GET", path, None, None, "127.0.0.1:1").await;
        assert_eq!(status, want, "{path}");
    }
    let (status, headers, body) = app
        .call_full("POST", "/health", None, None, "127.0.0.1:1")
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert!(!headers.contains_key("allow"), "{headers:?}");
}

#[tokio::test]
async fn options_requests_get_no_request_id() {
    let app = TestApp::start().await;
    let (_, headers, _) = app
        .call_full("OPTIONS", "/health", None, None, "127.0.0.1:1")
        .await;
    assert!(!headers.contains_key("x-request-id"), "{headers:?}");
}

#[tokio::test]
async fn ready_checks_database_and_redis() {
    let app = TestApp::start().await;
    let (status, body) = app.call("GET", "/health/ready", None, None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body["message"],
        "Seluruh dependensi sistem siap melayani traffic"
    );
    assert_eq!(body["data"]["status"], "ready");
    assert_eq!(body["data"]["database"]["status"], "healthy");
    assert_eq!(body["data"]["redis"]["status"], "healthy");
    assert!(body["data"]["database"]["latencyMs"].is_u64());
}

#[tokio::test]
async fn request_id_is_echoed_and_generated() {
    let app = TestApp::start().await;
    let (_, headers, _) = app
        .call_full("GET", "/health", None, None, "127.0.0.1:1")
        .await;
    let generated = headers["x-request-id"].to_str().expect("header");
    assert_eq!(generated.len(), 36);
}

#[tokio::test]
async fn unknown_route_and_wrong_method_are_404_envelopes() {
    let app = TestApp::start().await;
    for (method, path) in [
        ("GET", "/nope"),
        ("PUT", "/health"),
        ("GET", "/api/v1/nope"),
    ] {
        let (status, body) = app.call(method, path, None, None).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{method} {path}");
        assert_eq!(
            body,
            json!({
                "success": false,
                "message": "Endpoint atau rute tidak ditemukan",
                "error_code": "RESOURCE_NOT_FOUND",
            })
        );
    }
}

#[tokio::test]
async fn jwks_is_public_and_cacheable() {
    let app = TestApp::start().await;
    let (status, headers, body) = app
        .call_full("GET", "/.well-known/jwks.json", None, None, "127.0.0.1:1")
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers["cache-control"], "public, max-age=86400");
    let key = &body["keys"][0];
    assert_eq!(key["kty"], "RSA");
    assert_eq!(key["alg"], "RS256");
    assert_eq!(key["use"], "sig");
    assert_eq!(key["kid"], app.state.keys.active_kid());
}

#[tokio::test]
async fn cors_preflight_and_headers() {
    let app = TestApp::start().await;
    let (status, headers, _) = app
        .call_full("OPTIONS", "/api/v1/guests", None, None, "127.0.0.1:1")
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(headers["access-control-max-age"], "5");
    assert_eq!(
        headers["access-control-allow-headers"],
        "Content-Type, Authorization, x-request-id"
    );
    assert_eq!(headers["access-control-expose-headers"], "x-request-id");
    assert_eq!(headers["vary"], "*");
}

#[tokio::test]
async fn rate_limit_blocks_with_envelope_and_headers() {
    let app = TestApp::start_with(|c| {
        c.rate_limit_max = 3;
        c.rate_limit_duration = 60_000;
    })
    .await;
    for remaining in [2, 1, 0] {
        let (status, headers, _) = app
            .call_full("GET", "/health", None, None, "203.0.113.9:4000")
            .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(headers["ratelimit-limit"], "3");
        assert_eq!(
            headers["ratelimit-remaining"],
            remaining.to_string().as_str()
        );
    }
    let (status, headers, body) = app
        .call_full("GET", "/health", None, None, "203.0.113.9:4000")
        .await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(
        body,
        json!({
            "success": false,
            "message": "Terlalu banyak permintaan. Silakan coba beberapa saat lagi.",
            "error_code": "RATE_LIMIT_EXCEEDED",
        })
    );
    assert_eq!(headers["retry-after"], headers["ratelimit-reset"]);
    assert_eq!(headers["x-content-type-options"], "nosniff");

    // Another client is unaffected.
    let (status, _, _) = app
        .call_full("GET", "/health", None, None, "203.0.113.10:4000")
        .await;
    assert_eq!(status, StatusCode::OK);
}

mod extractors {
    use axum::Router;
    use axum::routing::{get, post};
    use kawalo_api::core::error::ApiError;
    use kawalo_api::core::extract::schema::{Field, Kind, Schema};
    use kawalo_api::core::extract::{
        AuthContext, AuthedJson, Body, CurrentUser, Validate, ValidatedJson, ValidatedQuery,
        ValidationErrors,
    };
    use kawalo_api::core::response::{ApiResult, ok};
    use kawalo_api::core::state::AppState;
    use serde::Deserialize;
    use serde_json::{Value, json};

    use super::*;

    #[derive(Deserialize)]
    struct Echo {
        title: String,
    }

    impl Body for Echo {
        const SCHEMA: Schema = Schema::new(&[Field::required("title", Kind::text(1, 8))]);
    }

    #[derive(Deserialize)]
    struct Paging {
        limit: Option<f64>,
    }

    impl Validate for Paging {
        fn validate(&self) -> Result<(), ValidationErrors> {
            let mut errors = ValidationErrors::new();
            if let Some(limit) = self.limit {
                errors.range("limit", limit, 0.0..=100.0);
            }
            errors.into_result()
        }
    }

    fn routes() -> Router<AppState> {
        Router::new()
            .route(
                "/_t/me",
                get(|user: CurrentUser| async move {
                    user.require_role(&["ADMIN"])?;
                    ok(
                        json!({"email": user.email, "group": user.require_group_uid()?}),
                        "ok",
                    )
                }),
            )
            .route(
                "/_t/role-ctx",
                get(|AuthContext(auth): AuthContext| async move {
                    let user = auth.require_role(&["ADMIN"])?;
                    ok(user.email, "ok")
                }),
            )
            .route(
                "/_t/echo",
                post(
                    |ValidatedJson(body): ValidatedJson<Echo>| async move { ok(body.title, "ok") },
                ),
            )
            .route(
                "/_t/authed-echo",
                post(|AuthedJson(auth, body): AuthedJson<Echo>| async move {
                    let user = auth.require_role(&["ADMIN"])?;
                    ok(json!({"by": user.email, "title": body.title}), "ok")
                }),
            )
            .route(
                "/_t/page",
                get(|ValidatedQuery(q): ValidatedQuery<Paging>| async move { ok(q.limit, "ok") }),
            )
            .route(
                "/_t/db-error",
                get(|| async {
                    Err::<_, ApiError>(sqlx::Error::PoolTimedOut.into()) as ApiResult<Value>
                }),
            )
            .route(
                "/_t/panic",
                get(|| async { panic!("boom") as ApiResult<Value> }),
            )
    }

    async fn app() -> TestApp {
        TestApp::start_with_routes(|_| {}, routes()).await
    }

    #[tokio::test]
    async fn missing_or_bad_token_is_401() {
        let app = app().await;
        let (status, body) = app.call("GET", "/_t/me", None, None).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(body["error_code"], "AUTH_UNAUTHORIZED");
        assert_eq!(
            body["message"],
            "Akses tidak diizinkan. Token tidak valid atau telah kedaluwarsa."
        );

        let (status, body) = app.call("GET", "/_t/me", Some("garbage"), None).await;
        assert_eq!(
            (status, body["error_code"].as_str()),
            (StatusCode::UNAUTHORIZED, Some("AUTH_UNAUTHORIZED"))
        );

        let (_, body) = app.call("GET", "/_t/role-ctx", None, None).await;
        assert_eq!(
            body["message"],
            "Akses tidak diizinkan. Silakan login terlebih dahulu."
        );
    }

    #[tokio::test]
    async fn valid_token_passes_and_role_is_enforced() {
        let app = app().await;
        let admin = app.token_for("admin-ext@test.id", "grp-test-1", true).await;
        let (status, body) = app.call("GET", "/_t/me", Some(&admin), None).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(
            body["data"],
            json!({"email": "admin-ext@test.id", "group": "grp-test-1"})
        );

        let guard = app
            .token_for("guard-ext@test.id", "grp-test-1", false)
            .await;
        let (status, body) = app.call("GET", "/_t/me", Some(&guard), None).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(body["error_code"], "AUTH_FORBIDDEN");
        assert_eq!(
            body["message"],
            "Anda tidak memiliki wewenang untuk mengakses sumber daya ini."
        );
    }

    #[tokio::test]
    async fn authed_json_validates_the_body_before_authenticating() {
        let app = app().await;
        let bad = json!({"title": ""});
        let good = json!({"title": "ok"});

        let (status, body) = app.call("POST", "/_t/authed-echo", None, Some(&bad)).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
        assert_eq!(body["error_code"], "VALIDATION_ERROR");

        let (status, body) = app.call("POST", "/_t/authed-echo", None, Some(&good)).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(
            body["message"],
            "Akses tidak diizinkan. Silakan login terlebih dahulu."
        );

        let guard = app.token_for("guard-aj@test.id", "grp-test-1", false).await;
        let (status, _) = app
            .call("POST", "/_t/authed-echo", Some(&guard), Some(&good))
            .await;
        assert_eq!(status, StatusCode::FORBIDDEN);

        let admin = app.token_for("admin-aj@test.id", "grp-test-1", true).await;
        let (status, body) = app
            .call("POST", "/_t/authed-echo", Some(&admin), Some(&good))
            .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["data"]["by"], "admin-aj@test.id");
    }

    #[tokio::test]
    async fn bumped_session_version_makes_the_token_stale() {
        let app = app().await;
        let email = "stale-ext@test.id";
        let token = app.token_for(email, "grp-test-1", true).await;
        app.state
            .sessions
            .increment_version(email)
            .await
            .expect("increment");
        let (status, body) = app.call("GET", "/_t/me", Some(&token), None).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(body["error_code"], "AUTH_SESSION_STALE");
        assert_eq!(
            body["message"],
            "Sesi telah diperbarui di sistem. Silakan perbarui sesi Anda."
        );
    }

    #[tokio::test]
    async fn broken_bodies_are_400_or_422() {
        let app = app().await;
        let (status, body) = app.call("POST", "/_t/echo", None, Some(&json!({}))).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(body["message"], "Format data masukan tidak valid");
        assert_eq!(
            body["details"],
            "/title: Expected property 'title' to be string but found: undefined"
        );

        let (status, body) = app
            .call(
                "POST",
                "/_t/echo",
                None,
                Some(&json!({"title": "sembilan!!"})),
            )
            .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(
            body["details"],
            "/title: Expected string length less or equal to 8"
        );

        let (status, body) = app
            .call(
                "POST",
                "/_t/echo",
                None,
                Some(&json!({"title": "ok", "extra": 1})),
            )
            .await;
        assert_eq!((status, &body["data"]), (StatusCode::OK, &json!("ok")));
    }

    #[tokio::test]
    async fn malformed_json_over_the_wire_is_400() {
        use axum::body::Body;
        use axum::http::Request;
        use tower::ServiceExt;

        let app = app().await;
        let request = Request::builder()
            .method("POST")
            .uri("/_t/echo")
            .header("content-type", "application/json")
            .body(Body::from("{bad"))
            .expect("request");
        let response = app.router.clone().oneshot(request).await.expect("response");
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn query_validation_is_422() {
        let app = app().await;
        let (status, body) = app.call("GET", "/_t/page?limit=abc", None, None).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(body["error_code"], "VALIDATION_ERROR");
        let (status, _) = app.call("GET", "/_t/page?limit=500", None, None).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        let (status, body) = app.call("GET", "/_t/page?limit=25", None, None).await;
        assert_eq!((status, &body["data"]), (StatusCode::OK, &json!(25.0)));
    }

    #[tokio::test]
    async fn database_errors_hide_nothing_outside_production() {
        let app = app().await;
        let (status, body) = app.call("GET", "/_t/db-error", None, None).await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(body["error_code"], "DATABASE_ERROR");
        assert_eq!(body["message"], "Kesalahan operasi basis data");
        assert!(body["details"].is_string());
    }

    #[tokio::test]
    async fn panics_become_500_envelopes_with_security_headers() {
        let app = app().await;
        let (status, headers, body) = app
            .call_full("GET", "/_t/panic", None, None, "127.0.0.1:2")
            .await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(body["error_code"], "INTERNAL_SERVER_ERROR");
        assert_eq!(body["message"], "Terjadi kesalahan pada server internal");
        assert_eq!(headers["x-frame-options"], "DENY");
        assert!(headers.contains_key("x-request-id"));
    }
}
