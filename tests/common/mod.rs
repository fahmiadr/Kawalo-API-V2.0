//! Helpers shared by the integration tests. They need the scratch services from
//! `scripts/dev-services.sh start`; `DATABASE_URL` / `REDIS_URL` override the scratch
//! addresses, and the developer's own default instances are never used implicitly.

#![allow(dead_code, reason = "each test binary uses a different subset")]

use std::net::SocketAddr;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{HeaderMap, Method, Request, StatusCode, header};
use http_body_util::BodyExt;
use kawalo_api::core::config::Config;
use kawalo_api::core::security::{JwtUserPayload, RsaKeyManager, sign_access_token};
use kawalo_api::core::state::AppState;
use kawalo_api::core::{db, redis};
use kawalo_api::{build_app, build_app_with};
use serde_json::Value;
use tempfile::TempDir;
use tower::ServiceExt;

pub const TEST_MODULUS_BITS: usize = 2048;
const SCRATCH_DATABASE_URL: &str = "mysql://root:@127.0.0.1:3380/kawalo_rust";
const SCRATCH_REDIS_URL: &str = "redis://127.0.0.1:6390";

/// A running application bound to the test database and Redis, with throwaway signing keys.
pub struct TestApp {
    pub state: AppState,
    pub router: Router,
    _certs: TempDir,
}

impl TestApp {
    /// Starts with the environment's configuration, which tests may adjust first.
    ///
    /// # Panics
    /// When the scratch services are not running or the environment variables are missing.
    pub async fn start() -> Self {
        Self::start_with(|_| {}).await
    }

    /// Like [`start`](Self::start), letting the test change the [`Config`] (for example the rate
    /// limit).
    ///
    /// # Panics
    /// Same as [`start`](Self::start).
    pub async fn start_with(configure: impl FnOnce(&mut Config)) -> Self {
        Self::start_with_routes(configure, Router::new()).await
    }

    /// Like [`start_with`](Self::start_with), adding root-level routes that only the test needs.
    ///
    /// # Panics
    /// Same as [`start`](Self::start).
    pub async fn start_with_routes(
        configure: impl FnOnce(&mut Config),
        extra: Router<AppState>,
    ) -> Self {
        let mut config = Config::from_env();
        config.node_env = "test".to_owned();
        if std::env::var_os("DATABASE_URL").is_none() {
            config.database_url = SCRATCH_DATABASE_URL.to_owned();
        }
        if std::env::var_os("REDIS_URL").is_none() {
            config.redis_url = SCRATCH_REDIS_URL.to_owned();
        }
        configure(&mut config);
        let config = Arc::new(config);

        let certs = tempfile::tempdir().expect("temp certs dir");
        let certs_dir = certs.path().to_owned();
        let keys = tokio::task::spawn_blocking(move || {
            RsaKeyManager::init_with_modulus(certs_dir, TEST_MODULUS_BITS)
        })
        .await
        .expect("key task")
        .expect("test keys");

        let pool = db::connect(&config)
            .await
            .expect("MariaDB reachable (run scripts/dev-services.sh start)");
        let redis = redis::connect(&config).await.expect("Redis reachable");
        let state = AppState::new(config, pool, redis, keys);
        let router = if extra.has_routes() {
            build_app_with(state.clone(), extra)
        } else {
            build_app(state.clone())
        };
        Self {
            state,
            router,
            _certs: certs,
        }
    }

    /// Mints an access token for `email` in `group`, using (and creating) its Redis session
    /// version like the login flow does. `admin` sets role `ADMIN` and `allow_admin`, otherwise
    /// the role is `GUARD`.
    ///
    /// # Panics
    /// On Redis or signing failures.
    pub async fn token_for(&self, email: &str, group: &str, admin: bool) -> String {
        let session_ver = self
            .state
            .sessions
            .get_or_init_version(email)
            .await
            .expect("session version");
        let payload = JwtUserPayload {
            sub: Some(email.to_owned()),
            email: Some(email.to_owned()),
            name: email.split('@').next().map(str::to_owned),
            role: Some(if admin { "ADMIN" } else { "GUARD" }.to_owned()),
            group_uid: Some(group.to_owned()),
            allow_admin: admin,
            session_ver,
            ..JwtUserPayload::default()
        };
        sign_access_token(&self.state.keys, self.state.config.jwt_expires_in, &payload)
            .expect("sign token")
            .token
    }

    /// Sends a JSON request and returns the status and parsed JSON body (`Null` when empty or not
    /// JSON). The peer address is `127.0.0.1`.
    ///
    /// # Panics
    /// When the request cannot be built or the body cannot be read.
    pub async fn call(
        &self,
        method: &str,
        path: &str,
        token: Option<&str>,
        body: Option<&Value>,
    ) -> (StatusCode, Value) {
        let (status, _, json) = self
            .call_full(method, path, token, body, "127.0.0.1:50000")
            .await;
        (status, json)
    }

    /// [`call`](Self::call) that also returns the response headers and lets the test choose the
    /// peer address.
    ///
    /// # Panics
    /// Same as [`call`](Self::call).
    pub async fn call_full(
        &self,
        method: &str,
        path: &str,
        token: Option<&str>,
        body: Option<&Value>,
        peer: &str,
    ) -> (StatusCode, HeaderMap, Value) {
        let mut builder = Request::builder()
            .method(Method::from_bytes(method.as_bytes()).expect("method"))
            .uri(path);
        if let Some(token) = token {
            builder = builder.header(header::AUTHORIZATION, format!("Bearer {token}"));
        }
        let request_body = match body {
            Some(json) => {
                builder = builder.header(header::CONTENT_TYPE, "application/json");
                Body::from(serde_json::to_vec(json).expect("serialise body"))
            }
            None => Body::empty(),
        };
        let mut request = builder.body(request_body).expect("request");
        request.extensions_mut().insert(ConnectInfo(
            peer.parse::<SocketAddr>().expect("peer address"),
        ));

        let response = self
            .router
            .clone()
            .oneshot(request)
            .await
            .expect("infallible router");
        let (parts, body) = response.into_parts();
        let bytes = body.collect().await.expect("response body").to_bytes();
        (
            parts.status,
            parts.headers,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    /// Serves the router on an ephemeral port, for tests that need real sockets (SSE, WebSocket).
    ///
    /// # Panics
    /// When no port can be bound.
    pub async fn serve(&self) -> SocketAddr {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let addr = listener.local_addr().expect("local addr");
        let app = self
            .router
            .clone()
            .into_make_service_with_connect_info::<SocketAddr>();
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        addr
    }
}
