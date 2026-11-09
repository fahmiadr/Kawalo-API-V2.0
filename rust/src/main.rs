//! Server entry point, ported from `server.ts`.

use std::env;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use kawalo_api::build_app;
use kawalo_api::core::config::Config;
use kawalo_api::core::constants::APP_NAME;
use kawalo_api::core::security::RsaKeyManager;
use kawalo_api::core::state::AppState;
use kawalo_api::core::{db, health, logging, redis};
use tokio::net::TcpListener;
use tokio::signal;

/// How long open connections (SSE, WebSocket) get to finish after a shutdown signal.
const SHUTDOWN_GRACE: Duration = Duration::from_secs(10);

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    health::mark_started();
    let config = Arc::new(Config::from_env());
    logging::init(&config).context("installing the log subscriber")?;
    tracing::info!("[BOOT] starting {APP_NAME} ({})", config.node_env);

    let certs_dir = env::current_dir()
        .context("reading the working directory")?
        .join("certs");
    let keys = tokio::task::spawn_blocking(move || RsaKeyManager::init(certs_dir))
        .await
        .context("key initialisation task")?
        .context("initialising RSA keys")?;
    keys.enable_file_watcher()
        .context("watching the certs directory")?;

    let pool = db::connect(&config)
        .await
        .context("connecting to MariaDB")?;
    let redis = redis::connect(&config)
        .await
        .context("connecting to Redis")?;
    let state = AppState::new(Arc::clone(&config), pool.clone(), redis, keys.clone());

    let listener = bind(config.port).await?;
    let addr = listener
        .local_addr()
        .context("reading the listen address")?;
    tracing::info!("[Server] listening on http://{addr}");
    tracing::info!("[JWKS] OIDC endpoint: http://{addr}/.well-known/jwks.json");
    tracing::info!(
        "[Health] liveness: http://{addr}/health, readiness: http://{addr}/health/ready"
    );

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let server = axum::serve(
        listener,
        build_app(state).into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(async {
        let _ = shutdown_rx.await;
    });
    let mut server = tokio::spawn(async move { server.await });

    tokio::select! {
        result = &mut server => {
            result.context("server task")?.context("serving requests")?;
        }
        signal = shutdown_signal() => {
            tracing::info!("[Server] received {signal}, shutting down gracefully");
            let _ = shutdown_tx.send(());
            if tokio::time::timeout(SHUTDOWN_GRACE, &mut server).await.is_err() {
                tracing::warn!("[Server] connections still open after {SHUTDOWN_GRACE:?}, closing anyway");
                server.abort();
            }
        }
    }

    keys.close_file_watcher();
    pool.close().await;
    tracing::info!("[Server] database and Redis connections closed");
    Ok(())
}

/// Dual-stack like Node's default listen, falling back to IPv4-only hosts.
async fn bind(port: u16) -> anyhow::Result<TcpListener> {
    match TcpListener::bind(("::", port)).await {
        Ok(listener) => Ok(listener),
        Err(_) => TcpListener::bind(("0.0.0.0", port))
            .await
            .with_context(|| format!("binding port {port}")),
    }
}

async fn shutdown_signal() -> &'static str {
    let ctrl_c = async {
        let _ = signal::ctrl_c().await;
        "SIGINT"
    };
    #[cfg(unix)]
    let terminate = async {
        match signal::unix::signal(signal::unix::SignalKind::terminate()) {
            Ok(mut stream) => {
                stream.recv().await;
                "SIGTERM"
            }
            Err(_) => std::future::pending::<&'static str>().await,
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<&'static str>();

    tokio::select! {
        name = ctrl_c => name,
        name = terminate => name,
    }
}
