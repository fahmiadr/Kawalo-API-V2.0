//! Per-stage latency of the authenticated request path against live services (sequential, 1 caller).
//! Run from the API root: `DATABASE_URL=.. REDIS_URL=.. NODE_ENV=production \
//!   cargo run --release --example auth_profile`

use std::hint::black_box;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;

use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::Request;
use http_body_util::BodyExt;
use kawalo_api::build_app;
use kawalo_api::core::config::Config;
use kawalo_api::core::db;
use kawalo_api::core::extract::authenticate_token;
use kawalo_api::core::redis as kw_redis;
use kawalo_api::core::security::{JwtUserPayload, RsaKeyManager, sign_access_token};
use kawalo_api::core::state::AppState;
use tower::ServiceExt;

async fn time<F: std::future::Future>(label: &str, iters: u32, mut f: impl FnMut() -> F) {
    for _ in 0..(iters / 10).max(1) {
        black_box(f().await);
    }
    let start = Instant::now();
    for _ in 0..iters {
        black_box(f().await);
    }
    let per = start.elapsed() / iters;
    println!("{label:<34} {:>9.1} us/op", per.as_secs_f64() * 1e6);
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = Arc::new(Config::from_env());
    let keys = RsaKeyManager::init(std::env::current_dir()?.join("certs"))?;
    let pool = db::connect(&config).await?;
    let redis = kw_redis::connect(&config).await?;
    let state = AppState::new(Arc::clone(&config), pool.clone(), redis, keys.clone());

    let conn = state.redis.clone();
    let _: i64 = state.sessions.get_or_init_version("1").await?;
    let ver = state.sessions.get_or_init_version("1").await?;
    let payload = JwtUserPayload {
        sub: Some("1".into()),
        email: Some("bench-user-1@bench.test".into()),
        name: Some("bench-user-1".into()),
        role: Some("GUARD".into()),
        group_uid: Some("bench-g1".into()),
        allow_patrol: true,
        session_ver: ver,
        ..JwtUserPayload::default()
    };
    let token = sign_access_token(&keys, 900, &payload)?.token;

    time("raw redis GET", 5000, || {
        let mut conn = conn.clone();
        async move {
            redis::cmd("GET")
                .arg("auth:user:1:session_ver")
                .query_async::<Option<String>>(&mut conn)
                .await
        }
    })
    .await;
    time("sqlx SELECT 1 (pooled)", 3000, || async {
        sqlx::query("SELECT 1").execute(&pool).await
    })
    .await;
    time("sqlx guests main query", 1000, || async {
        sqlx::query(
            "SELECT * FROM tkguest WHERE GuestGroupUID = 'bench-g1' ORDER BY GuestDT DESC LIMIT 20",
        )
        .fetch_all(&pool)
        .await
    })
    .await;
    time("authenticate_token (jwt+redis)", 3000, || async {
        authenticate_token(&state, &token).await
    })
    .await;

    let app = build_app(state.clone());
    for path in [
        "/api/v1/guests?limit=20",
        "/api/v1/information?limit=20",
        "/api/v1/attendance?limit=20",
        "/health",
    ] {
        time(&format!("oneshot GET {path}"), 1000, || async {
            let mut request = Request::builder()
                .uri(path)
                .header("authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .expect("request");
            request.extensions_mut().insert(ConnectInfo(
                "127.0.0.1:5000".parse::<SocketAddr>().expect("addr"),
            ));
            let response = app.clone().oneshot(request).await.expect("infallible");
            response
                .into_body()
                .collect()
                .await
                .expect("body")
                .to_bytes()
                .len()
        })
        .await;
    }
    Ok(())
}
