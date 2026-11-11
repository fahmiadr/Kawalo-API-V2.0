//! Server-side speech-to-text for push-to-talk turns that have no transcript yet.
//!
//! The speaker's device (Android 13+) usually uploads the text itself; this worker fills the
//! rest: rows the device marked `unsupported`/`failed`, and rows nobody reported on within the
//! grace period. It runs as its own process (`src/bin/ptt_transcriber.rs`), one job at a time:
//! claim a row, decode the Ogg/Opus file to a temporary 16 kHz WAV, skip near-silence, run
//! `whisper-cli`, and write the text only where the transcript is still empty, so a device text
//! that arrives meanwhile always wins. See `docs/ptt-transcriber.md` for operations.

mod audio;
mod queue;
mod whisper;

use std::path::{Component, Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{Context, bail};
use sqlx::MySqlPool;
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;

use queue::{Claim, Scope};
use whisper::{Invocation, WhisperError};

/// Same cap as device transcripts.
const MAX_TEXT_CHARS: usize = 2000;
const RECOVERY_EVERY: Duration = Duration::from_secs(60);
const MIN_BACKOFF: Duration = Duration::from_secs(1);
const MAX_BACKOFF: Duration = Duration::from_secs(60);

/// Worker settings. `from_env` documents each variable and its default.
#[derive(Debug, Clone)]
pub struct Settings {
    /// `UPLOAD_DIR` of the API (shared config); recordings live under `ptt/`.
    pub upload_dir: PathBuf,
    /// `STT_BIN_PATH`, default `whisper-cli` (looked up in `PATH`).
    pub bin_path: PathBuf,
    /// `STT_MODEL_PATH`, required: the ggml model file, e.g. `ggml-small-q5_1.bin`.
    pub model_path: PathBuf,
    /// `STT_VAD_MODEL_PATH`, optional: a whisper.cpp VAD model (`ggml-silero-*.bin`); when set,
    /// `--vad -vm <path>` is passed so Whisper skips non-speech inside a turn.
    pub vad_model_path: Option<PathBuf>,
    /// `STT_GRACE_SECONDS`, default 60: how long a row without any device report waits before the
    /// server takes it, and how long a server failure waits before the next attempt.
    pub grace: Duration,
    /// `STT_THREADS`, default 1.
    pub threads: u32,
    /// `STT_LANGUAGE`, default `id`.
    pub language: String,
    /// `STT_MIN_SPEECH_MS`, default 1000: turns with less loud audio are `no_speech` and never
    /// reach Whisper.
    pub min_speech_ms: u64,
    /// `STT_SILENCE_DBFS`, default -40: the RMS level of a 20 ms frame that counts as speech for
    /// the gate above. Room noise through the phone's voice-communication path is usually around
    /// -55 to -60 dBFS and speech -30 to -15; tune it with real turns.
    pub silence_dbfs: f32,
    /// `STT_JOB_TIMEOUT_SECONDS`, default 120: whisper-cli is killed after this.
    pub job_timeout: Duration,
    /// `STT_POLL_SECONDS`, default 5: wait between queue checks when the queue is empty.
    pub poll: Duration,
    /// `STT_STALE_CLAIM_SECONDS`, default 300: a `processing` row older than this is assumed to
    /// belong to a dead worker. Keep it above the job timeout.
    pub stale_claim: Duration,
    /// `STT_MAX_ATTEMPTS`, default 3: after this many claims a row ends as `error`.
    pub max_attempts: u8,
    /// Parent of the per-job temporary directories (`TMPDIR`, else `/tmp`).
    pub work_dir: PathBuf,
    /// Restricts the worker to one group. For tests that share a database; `None` in production.
    #[doc(hidden)]
    pub only_group: Option<String>,
}

impl Settings {
    /// # Errors
    /// When `STT_MODEL_PATH` is missing or a variable does not parse.
    pub fn from_env(upload_dir: PathBuf) -> anyhow::Result<Self> {
        Self::from_lookup(upload_dir, |key| std::env::var(key).ok())
    }

    fn from_lookup(
        upload_dir: PathBuf,
        get: impl Fn(&str) -> Option<String>,
    ) -> anyhow::Result<Self> {
        let get = |key: &str| {
            get(key)
                .map(|v| v.trim().to_owned())
                .filter(|v| !v.is_empty())
        };
        let num = |key: &str, default: u64| -> anyhow::Result<u64> {
            get(key).map_or(Ok(default), |v| {
                v.parse()
                    .with_context(|| format!("{key} must be a whole number, got {v:?}"))
            })
        };
        let secs =
            |key: &str, default: u64| num(key, default).map(|n| Duration::from_secs(n.max(1)));
        let Some(model_path) = get("STT_MODEL_PATH") else {
            bail!("STT_MODEL_PATH is required (path of the whisper.cpp ggml model)");
        };
        let silence_dbfs = match get("STT_SILENCE_DBFS") {
            None => -40.0,
            Some(v) => match v.parse::<f32>() {
                Ok(db) if (-120.0..=0.0).contains(&db) => db,
                _ => bail!("STT_SILENCE_DBFS must be a number between -120 and 0, got {v:?}"),
            },
        };
        Ok(Self {
            upload_dir,
            bin_path: get("STT_BIN_PATH")
                .unwrap_or_else(|| "whisper-cli".to_owned())
                .into(),
            model_path: model_path.into(),
            vad_model_path: get("STT_VAD_MODEL_PATH").map(PathBuf::from),
            grace: Duration::from_secs(num("STT_GRACE_SECONDS", 60)?),
            threads: u32::try_from(num("STT_THREADS", 1)?.max(1)).unwrap_or(1),
            language: get("STT_LANGUAGE").unwrap_or_else(|| "id".to_owned()),
            min_speech_ms: num("STT_MIN_SPEECH_MS", 1000)?,
            silence_dbfs,
            job_timeout: secs("STT_JOB_TIMEOUT_SECONDS", 120)?,
            poll: secs("STT_POLL_SECONDS", 5)?,
            stale_claim: secs("STT_STALE_CLAIM_SECONDS", 300)?,
            max_attempts: u8::try_from(num("STT_MAX_ATTEMPTS", 3)?.clamp(1, 255)).unwrap_or(3),
            work_dir: std::env::temp_dir(),
            only_group: None,
        })
    }

    /// `TranscriptLocale` written with server text, in the device's `xx_YY` form.
    pub fn locale(&self) -> String {
        match self.language.as_str() {
            "id" => "id_ID".to_owned(),
            other => other.to_owned(),
        }
    }

    fn scope(&self) -> Scope<'_> {
        Scope {
            max_attempts: self.max_attempts,
            grace_secs: self.grace.as_secs(),
            stale_secs: self.stale_claim.as_secs(),
            only_group: self.only_group.as_deref(),
        }
    }
}

/// What happened to one claimed recording.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Server text stored (`done`, source `server`).
    Done,
    NoSpeech,
    /// A device text or a delete arrived first; nothing was written.
    Superseded,
    /// Will be retried (`failed`).
    Failed,
    /// Out of attempts or unusable (`error`).
    Error,
    /// Handed back without using an attempt (shutdown, whisper-cli could not start).
    Released,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Processed {
    pub id: String,
    pub outcome: Outcome,
}

enum Job {
    Text(String),
    NoSpeech,
}

enum JobError {
    /// Nothing about the row can change this (unsafe path).
    Fatal(String),
    Retry(String),
    /// The worker cannot run whisper at all; the row is not to blame.
    Release(String),
}

pub struct Worker {
    db: MySqlPool,
    settings: Settings,
}

impl Worker {
    pub fn new(db: MySqlPool, settings: Settings) -> Self {
        Self { db, settings }
    }

    /// Recovers stale claims, then polls until `cancel` fires. Never returns early on errors:
    /// database failures are logged and retried with an exponential backoff (sqlx reconnects).
    pub async fn run(&self, cancel: CancellationToken) {
        let s = &self.settings;
        tracing::info!(
            "[STT] worker started (model {}, language {}, {} thread(s), grace {}s)",
            s.model_path.display(),
            s.language,
            s.threads,
            s.grace.as_secs()
        );
        let mut next_recovery = Instant::now();
        let mut backoff = MIN_BACKOFF;
        while !cancel.is_cancelled() {
            if Instant::now() >= next_recovery {
                match self.recover_stale().await {
                    Ok(0) => {}
                    Ok(n) => tracing::info!("[STT] returned {n} stale claim(s) to the queue"),
                    Err(err) => tracing::warn!("[STT] stale-claim recovery failed: {err}"),
                }
                next_recovery = Instant::now() + RECOVERY_EVERY;
            }
            let wait = match self.run_once(&cancel).await {
                Ok(Some(done)) if done.outcome != Outcome::Released => {
                    backoff = MIN_BACKOFF;
                    continue;
                }
                Ok(_) => s.poll,
                Err(err) => {
                    tracing::warn!("[STT] database error: {err}; retrying in {backoff:?}");
                    let wait = backoff;
                    backoff = (backoff * 2).min(MAX_BACKOFF);
                    wait
                }
            };
            tokio::select! {
                () = cancel.cancelled() => {}
                () = tokio::time::sleep(wait) => {}
            }
        }
        tracing::info!("[STT] worker stopped");
    }

    /// # Errors
    /// Database errors.
    pub async fn recover_stale(&self) -> sqlx::Result<u64> {
        queue::recover_stale(&self.db, self.settings.scope()).await
    }

    /// Claims and processes at most one recording. `Ok(None)` means the queue is empty. When
    /// `cancel` fires mid-job, whisper-cli is killed and the claim released.
    ///
    /// # Errors
    /// Database errors; a claim left `processing` by one is picked up by stale recovery.
    pub async fn run_once(&self, cancel: &CancellationToken) -> sqlx::Result<Option<Processed>> {
        let Some(claim) = queue::claim(&self.db, self.settings.scope()).await? else {
            return Ok(None);
        };
        let started = Instant::now();
        let outcome = tokio::select! {
            () = cancel.cancelled() => {
                queue::release(&self.db, &claim).await?;
                Outcome::Released
            }
            job = self.process(&claim) => self.settle(&claim, job).await?,
        };
        tracing::info!(
            "[STT] {} attempt {}: {outcome:?} in {} ms",
            claim.id,
            claim.attempt,
            started.elapsed().as_millis()
        );
        Ok(Some(Processed {
            id: claim.id,
            outcome,
        }))
    }

    async fn settle(&self, claim: &Claim, job: Result<Job, JobError>) -> sqlx::Result<Outcome> {
        // Reasons never contain audio or text: transcripts are personal data.
        Ok(match job {
            Ok(Job::Text(text)) => {
                if queue::store_text(&self.db, claim, &text, &self.settings.locale()).await? {
                    Outcome::Done
                } else {
                    queue::release(&self.db, claim).await?;
                    Outcome::Superseded
                }
            }
            Ok(Job::NoSpeech) => {
                queue::settle(&self.db, claim, "no_speech").await?;
                Outcome::NoSpeech
            }
            Err(JobError::Fatal(reason)) => {
                tracing::warn!("[STT] {}: {reason}; giving up", claim.id);
                queue::settle(&self.db, claim, "error").await?;
                Outcome::Error
            }
            Err(JobError::Retry(reason)) => {
                let (status, outcome) = if claim.attempt < self.settings.max_attempts {
                    ("failed", Outcome::Failed)
                } else {
                    ("error", Outcome::Error)
                };
                tracing::warn!(
                    "[STT] {} attempt {}/{}: {reason}",
                    claim.id,
                    claim.attempt,
                    self.settings.max_attempts
                );
                queue::settle(&self.db, claim, status).await?;
                outcome
            }
            Err(JobError::Release(reason)) => {
                tracing::warn!("[STT] {reason}; job released");
                queue::release(&self.db, claim).await?;
                Outcome::Released
            }
        })
    }

    async fn process(&self, claim: &Claim) -> Result<Job, JobError> {
        let s = &self.settings;
        let path = resolve(&s.upload_dir, &claim.file_path)
            .ok_or_else(|| JobError::Fatal("file path is not a plain relative path".to_owned()))?;
        let file = tokio::fs::read(&path)
            .await
            .map_err(|err| JobError::Retry(format!("reading the recording failed: {err}")))?;

        let (min_speech, dbfs, work_dir) = (s.min_speech_ms, s.silence_dbfs, s.work_dir.clone());
        // The temporary directory (0700, holding a 0600 WAV) is removed when `prepared` drops,
        // including when the job future is dropped on shutdown.
        let prepared =
            tokio::task::spawn_blocking(move || -> Result<Option<(TempDir, PathBuf)>, String> {
                let pcm = audio::decode(&file).map_err(|err| format!("decoding failed: {err}"))?;
                if audio::speech_ms(&pcm, dbfs) < min_speech {
                    return Ok(None);
                }
                let dir = tempfile::Builder::new()
                    .prefix("kawalo-stt-")
                    .tempdir_in(&work_dir)
                    .map_err(|err| format!("creating the work directory failed: {err}"))?;
                let wav = dir.path().join("turn.wav");
                audio::write_wav(&wav, &pcm)
                    .map_err(|err| format!("writing the WAV failed: {err}"))?;
                Ok(Some((dir, wav)))
            })
            .await
            .map_err(|err| JobError::Retry(format!("decoder task failed: {err}")))?
            .map_err(JobError::Retry)?;
        let Some((_dir, wav)) = prepared else {
            return Ok(Job::NoSpeech);
        };

        let raw = whisper::run(&Invocation {
            bin: &s.bin_path,
            model: &s.model_path,
            vad_model: s.vad_model_path.as_deref(),
            wav: &wav,
            language: &s.language,
            threads: s.threads,
            timeout: s.job_timeout,
        })
        .await
        .map_err(|err| match err {
            WhisperError::Spawn(_) => JobError::Release(err.to_string()),
            _ => JobError::Retry(err.to_string()),
        })?;
        let text = whisper::clean(&raw, MAX_TEXT_CHARS);
        Ok(if text.is_empty() {
            Job::NoSpeech
        } else {
            Job::Text(text)
        })
    }
}

/// Same rule as the audio endpoint: only plain path segments below `root`.
fn resolve(root: &Path, relative: &str) -> Option<PathBuf> {
    let path = Path::new(relative);
    (path.components().count() > 0 && path.components().all(|c| matches!(c, Component::Normal(_))))
        .then(|| root.join(path))
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    fn settings(vars: &[(&str, &str)]) -> anyhow::Result<Settings> {
        let vars: HashMap<String, String> = vars
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        Settings::from_lookup("/up".into(), |k| vars.get(k).cloned())
    }

    #[test]
    fn defaults_follow_the_documentation() {
        let s = settings(&[("STT_MODEL_PATH", "/m.bin")]).unwrap();
        assert_eq!(s.bin_path, PathBuf::from("whisper-cli"));
        assert_eq!(s.model_path, PathBuf::from("/m.bin"));
        assert_eq!(s.vad_model_path, None);
        assert_eq!(s.grace, Duration::from_secs(60));
        assert_eq!(s.threads, 1);
        assert_eq!(s.language, "id");
        assert_eq!(s.locale(), "id_ID");
        assert_eq!(s.min_speech_ms, 1000);
        assert_eq!(s.silence_dbfs, -40.0);
        assert_eq!(s.job_timeout, Duration::from_secs(120));
        assert_eq!(s.poll, Duration::from_secs(5));
        assert_eq!(s.stale_claim, Duration::from_secs(300));
        assert_eq!(s.max_attempts, 3);
        assert_eq!(s.only_group, None);
    }

    #[test]
    fn model_is_required_and_bad_numbers_are_rejected() {
        assert!(settings(&[]).is_err());
        assert!(settings(&[("STT_MODEL_PATH", "  ")]).is_err());
        assert!(settings(&[("STT_MODEL_PATH", "/m"), ("STT_GRACE_SECONDS", "1m")]).is_err());
        assert!(settings(&[("STT_MODEL_PATH", "/m"), ("STT_SILENCE_DBFS", "3")]).is_err());
        let s = settings(&[
            ("STT_MODEL_PATH", "/m"),
            ("STT_LANGUAGE", "en"),
            ("STT_VAD_MODEL_PATH", "/vad.bin"),
            ("STT_MAX_ATTEMPTS", "0"),
            ("STT_GRACE_SECONDS", "0"),
        ])
        .unwrap();
        assert_eq!(s.locale(), "en");
        assert_eq!(s.vad_model_path, Some(PathBuf::from("/vad.bin")));
        assert_eq!(s.max_attempts, 1);
        assert_eq!(s.grace, Duration::ZERO);
    }

    #[test]
    fn only_plain_relative_paths_resolve() {
        let root = Path::new("/up");
        assert_eq!(
            resolve(root, "ptt/2026/11/a.opus"),
            Some(PathBuf::from("/up/ptt/2026/11/a.opus"))
        );
        for bad in ["", "/etc/passwd", "../x", "ptt/../../x", "./x"] {
            assert_eq!(resolve(root, bad), None, "{bad}");
        }
    }
}
