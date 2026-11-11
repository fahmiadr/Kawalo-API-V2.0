//! `ptt_transcriber`: fills empty push-to-talk transcripts with whisper.cpp. Runs beside the API
//! as its own systemd service (`deploy/kawalo-ptt-transcriber.service`); operations in
//! `docs/ptt-transcriber.md`. Reads the same environment as the API (`DATABASE_URL`,
//! `UPLOAD_DIR`, `.env.*` in the working directory) plus the `STT_*` variables.

use anyhow::Context;
use kawalo_api::core::config::Config;
use kawalo_api::core::{db, logging};
use kawalo_api::features::ptt::transcriber::{Settings, Worker};
use tokio::signal;
use tokio_util::sync::CancellationToken;

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    let config = Config::from_env();
    logging::init(&config).context("installing the log subscriber")?;
    let settings = Settings::from_env(config.upload_dir.clone())?;
    // Without the model every job would fail and burn its attempts; refuse to start instead.
    anyhow::ensure!(
        settings.model_path.is_file(),
        "STT_MODEL_PATH {} is not a readable file",
        settings.model_path.display()
    );
    let pool = db::connect(&config)
        .await
        .context("connecting to the database")?;

    let cancel = CancellationToken::new();
    tokio::spawn({
        let cancel = cancel.clone();
        async move {
            let name = shutdown_signal().await;
            tracing::info!("[STT] received {name}, releasing the current job");
            cancel.cancel();
        }
    });
    Worker::new(pool.clone(), settings).run(cancel).await;
    pool.close().await;
    Ok(())
}

async fn shutdown_signal() -> &'static str {
    let terminate = async {
        match signal::unix::signal(signal::unix::SignalKind::terminate()) {
            Ok(mut stream) => {
                stream.recv().await;
                "SIGTERM"
            }
            Err(_) => std::future::pending().await,
        }
    };
    tokio::select! {
        _ = signal::ctrl_c() => "SIGINT",
        name = terminate => name,
    }
}
