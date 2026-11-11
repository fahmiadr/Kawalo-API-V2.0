//! Server-side PTT transcription worker against the scratch MariaDB (`scripts/dev-services.sh
//! start`) with a fake `whisper-cli` shell script. Each test works in its own group and the
//! worker is scoped to it, so rows of other tests in the shared table are never touched.

use std::collections::HashSet;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use kawalo_api::core::config::Config;
use kawalo_api::core::db;
use kawalo_api::features::ptt::recording::{self, TurnFrame};
use kawalo_api::features::ptt::transcriber::{Outcome, Settings, Worker};
use sqlx::MySqlPool;
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

const SCRATCH_DATABASE_URL: &str = "mysql://root:@127.0.0.1:3380/kawalo_rust";

struct Rig {
    db: MySqlPool,
    group: String,
    uploads: TempDir,
    work: TempDir,
    tools: TempDir,
    seq: std::sync::atomic::AtomicU32,
}

#[derive(Debug, PartialEq, sqlx::FromRow)]
struct Row {
    transcript: Option<String>,
    locale: Option<String>,
    source: Option<String>,
    status: Option<String>,
    attempts: u8,
    claimed: bool,
    transcript_at: bool,
}

impl Rig {
    async fn new() -> Self {
        let config = Config {
            database_url: std::env::var("DATABASE_URL")
                .unwrap_or_else(|_| SCRATCH_DATABASE_URL.to_owned()),
            ..Config::default()
        };
        let db = db::connect(&config).await.expect("scratch database");
        Self {
            db,
            group: format!("stt-{}", Uuid::new_v4().simple()),
            uploads: tempfile::tempdir().expect("uploads"),
            work: tempfile::tempdir().expect("work"),
            tools: tempfile::tempdir().expect("tools"),
            seq: 0.into(),
        }
    }

    fn log(&self) -> PathBuf {
        self.tools.path().join("calls.log")
    }

    fn calls(&self) -> Vec<String> {
        std::fs::read_to_string(self.log())
            .map(|s| s.lines().map(str::to_owned).collect())
            .unwrap_or_default()
    }

    /// A fake whisper-cli: logs its arguments, checks the WAV is there, then runs `body`.
    fn fake(&self, body: &str) -> PathBuf {
        let path = self
            .tools
            .path()
            .join(format!("whisper-{}", Uuid::new_v4().simple()));
        let script = format!(
            "#!/bin/sh\n[ -n \"$WARM\" ] && exit 0\nprintf '%s\\n' \"$*\" >> '{log}'\ntest -s \"$4\" || exit 3\n{body}\n",
            log = self.log().display()
        );
        std::fs::write(&path, script).expect("write fake");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        // macOS vets a new executable on its first exec, which can take longer than a test's
        // timeout; pay that cost here.
        std::process::Command::new(&path)
            .env("WARM", "1")
            .status()
            .expect("warm up fake");
        path
    }

    fn settings(&self, bin: &Path) -> Settings {
        let model = self.tools.path().join("model.bin");
        std::fs::write(&model, b"fake").expect("model");
        Settings {
            upload_dir: self.uploads.path().to_owned(),
            bin_path: bin.to_owned(),
            model_path: model,
            vad_model_path: None,
            grace: Duration::from_secs(60),
            threads: 1,
            language: "id".to_owned(),
            min_speech_ms: 1000,
            silence_dbfs: -40.0,
            job_timeout: Duration::from_secs(10),
            poll: Duration::from_secs(1),
            stale_claim: Duration::from_secs(300),
            max_attempts: 3,
            work_dir: self.work.path().to_owned(),
            only_group: Some(self.group.clone()),
        }
    }

    fn worker(&self, bin: &Path) -> Worker {
        Worker::new(self.db.clone(), self.settings(bin))
    }

    fn write_audio(&self, pcm: &[i16]) -> String {
        let relative = format!("ptt/2026/11/{}.opus", Uuid::new_v4());
        let full = self.uploads.path().join(&relative);
        std::fs::create_dir_all(full.parent().expect("parent")).expect("dirs");
        std::fs::write(&full, encode(pcm)).expect("audio");
        relative
    }

    /// Inserts a recording created `age_secs` ago. Rows are ordered by insertion through
    /// `StartedAt`.
    async fn insert(&self, file_path: &str, age_secs: u64, status: Option<&str>) -> String {
        let id = format!("rec_{}", Uuid::new_v4().simple());
        let n = self.seq.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        sqlx::query(
            "INSERT INTO tkpttrecording (RecordingUID, GroupUID, SpeakerUserId, SpeakerName, \
             SpeakerRole, StartedAt, DurationMs, SizeBytes, FilePath, CreatedAt, TranscriptStatus) \
             VALUES (?, ?, 'u1', 'Guard', 'GUARD', \
             NOW(3) - INTERVAL 1 DAY + INTERVAL ? SECOND, 2000, 100, ?, \
             NOW() - INTERVAL ? SECOND, ?)",
        )
        .bind(&id)
        .bind(&self.group)
        .bind(n)
        .bind(file_path)
        .bind(age_secs)
        .bind(status)
        .execute(&self.db)
        .await
        .expect("insert");
        id
    }

    async fn exec(&self, sql: &str, id: &str) {
        sqlx::query(sqlx::AssertSqlSafe(sql.to_owned()))
            .bind(id)
            .execute(&self.db)
            .await
            .expect("update");
    }

    async fn row(&self, id: &str) -> Row {
        sqlx::query_as(
            "SELECT Transcript AS transcript, TranscriptLocale AS locale, \
             TranscriptSource AS source, TranscriptStatus AS status, \
             TranscriptAttempts AS attempts, TranscriptClaimedAt IS NOT NULL AS claimed, \
             TranscriptAt IS NOT NULL AS transcript_at \
             FROM tkpttrecording WHERE RecordingUID = ?",
        )
        .bind(id)
        .fetch_one(&self.db)
        .await
        .expect("row")
    }

    fn work_is_empty(&self) -> bool {
        std::fs::read_dir(self.work.path())
            .expect("work dir")
            .count()
            == 0
    }

    /// Runs the worker until the queue is empty; returns the processed ids in order.
    async fn drain(&self, worker: &Worker) -> Vec<(String, Outcome)> {
        let cancel = CancellationToken::new();
        let mut done = Vec::new();
        while let Some(p) = worker.run_once(&cancel).await.expect("run_once") {
            done.push((p.id, p.outcome));
            assert!(done.len() < 100, "worker loops");
        }
        done
    }
}

fn tone(ms: usize) -> Vec<i16> {
    (0..ms * 16)
        .map(|n| (8000.0 * (n as f64 * 440.0 * std::f64::consts::TAU / 16_000.0).sin()) as i16)
        .collect()
}

fn encode(pcm: &[i16]) -> Vec<u8> {
    let payloads: Vec<Vec<u8>> = pcm
        .chunks(320)
        .map(|c| {
            let mut frame = [0i16; 320];
            frame[..c.len()].copy_from_slice(c);
            frame.iter().flat_map(|s| s.to_le_bytes()).collect()
        })
        .collect();
    let frames = payloads.iter().enumerate().map(|(i, p)| TurnFrame {
        sequence: i as u16,
        offset_ms: i as u32 * 20,
        payload: p,
    });
    recording::encode_turn(frames, 1, recording::Config::default()).expect("encode")
}

const SAYS: &str = "echo ' [BLANK_AUDIO] Pos dua   aman (musik) '";

#[tokio::test]
async fn success_stores_cleaned_server_text_and_removes_temp_files() {
    let rig = Rig::new().await;
    let id = rig
        .insert(&rig.write_audio(&tone(2000)), 0, Some("unsupported"))
        .await;
    let worker = rig.worker(&rig.fake(SAYS));

    assert_eq!(rig.drain(&worker).await, vec![(id.clone(), Outcome::Done)]);
    assert_eq!(
        rig.row(&id).await,
        Row {
            transcript: Some("Pos dua aman".to_owned()),
            locale: Some("id_ID".to_owned()),
            source: Some("server".to_owned()),
            status: Some("done".to_owned()),
            attempts: 1,
            claimed: true,
            transcript_at: true,
        }
    );
    let calls = rig.calls();
    assert_eq!(calls.len(), 1);
    let args: Vec<&str> = calls[0].split(' ').collect();
    assert_eq!(args[0], "-m");
    assert_eq!(args[2], "-f");
    assert!(args[3].ends_with("turn.wav"));
    assert_eq!(&args[4..], ["-l", "id", "-t", "1", "-nt", "-np"]);
    assert!(rig.work_is_empty(), "temporary WAV directory removed");
}

#[tokio::test]
async fn vad_model_adds_the_vad_flags() {
    let rig = Rig::new().await;
    rig.insert(&rig.write_audio(&tone(2000)), 0, Some("failed"))
        .await;
    let mut settings = rig.settings(&rig.fake(SAYS));
    settings.vad_model_path = Some("/models/vad.bin".into());
    let worker = Worker::new(rig.db.clone(), settings);
    rig.drain(&worker).await;
    assert!(rig.calls()[0].ends_with("-nt -np --vad -vm /models/vad.bin"));
}

#[tokio::test]
async fn only_eligible_rows_are_taken_oldest_first() {
    let rig = Rig::new().await;
    let audio = rig.write_audio(&tone(1500));
    let young_silent = rig.insert(&audio, 5, None).await;
    let old_silent = rig.insert(&audio, 120, None).await;
    let unsupported = rig.insert(&audio, 0, Some("unsupported")).await;
    let failed = rig.insert(&audio, 0, Some("failed")).await;
    let device = rig.insert(&audio, 120, Some("done")).await;
    rig.exec(
        "UPDATE tkpttrecording SET Transcript = 'dari HP', TranscriptSource = 'device' \
         WHERE RecordingUID = ?",
        &device,
    )
    .await;
    let deleted = rig.insert(&audio, 120, Some("unsupported")).await;
    rig.exec(
        "UPDATE tkpttrecording SET DeletedAt = NOW() WHERE RecordingUID = ?",
        &deleted,
    )
    .await;
    let exhausted = rig.insert(&audio, 120, Some("failed")).await;
    rig.exec(
        "UPDATE tkpttrecording SET TranscriptAttempts = 3 WHERE RecordingUID = ?",
        &exhausted,
    )
    .await;
    let mut finished = Vec::new();
    for status in ["no_speech", "error", "processing"] {
        finished.push(rig.insert(&audio, 120, Some(status)).await);
    }

    let worker = rig.worker(&rig.fake(SAYS));
    let done: Vec<String> = rig
        .drain(&worker)
        .await
        .into_iter()
        .map(|(id, _)| id)
        .collect();
    assert_eq!(done, vec![old_silent, unsupported, failed]);

    assert_eq!(rig.row(&young_silent).await.status, None, "grace respected");
    assert_eq!(
        rig.row(&device).await.transcript.as_deref(),
        Some("dari HP")
    );
    assert_eq!(rig.row(&deleted).await.attempts, 0);
    assert_eq!(rig.row(&exhausted).await.status.as_deref(), Some("failed"));
    for id in finished {
        assert_eq!(rig.row(&id).await.attempts, 0);
    }

    // Once the grace has passed the silent row is taken too.
    let mut settings = rig.settings(&rig.fake(SAYS));
    settings.grace = Duration::from_secs(1);
    let worker = Worker::new(rig.db.clone(), settings);
    tokio::time::sleep(Duration::from_millis(2100)).await;
    let done: Vec<String> = rig
        .drain(&worker)
        .await
        .into_iter()
        .map(|(id, _)| id)
        .collect();
    assert_eq!(done, vec![young_silent]);
}

#[tokio::test]
async fn silence_and_short_turns_are_no_speech_without_running_whisper() {
    let rig = Rig::new().await;
    let silent = rig
        .insert(&rig.write_audio(&vec![0; 48_000]), 0, Some("failed"))
        .await;
    let short = rig
        .insert(&rig.write_audio(&tone(600)), 0, Some("failed"))
        .await;
    let worker = rig.worker(&rig.fake(SAYS));

    assert_eq!(
        rig.drain(&worker).await,
        vec![
            (silent.clone(), Outcome::NoSpeech),
            (short.clone(), Outcome::NoSpeech)
        ]
    );
    assert!(rig.calls().is_empty(), "whisper-cli never started");
    for id in [silent, short] {
        let row = rig.row(&id).await;
        assert_eq!(
            (row.status.as_deref(), row.transcript),
            (Some("no_speech"), None)
        );
    }
    assert!(rig.work_is_empty());
}

#[tokio::test]
async fn whisper_output_with_only_tags_is_no_speech() {
    let rig = Rig::new().await;
    let id = rig
        .insert(&rig.write_audio(&tone(2000)), 0, Some("failed"))
        .await;
    let worker = rig.worker(&rig.fake("echo '[BLANK_AUDIO]'"));
    assert_eq!(
        rig.drain(&worker).await,
        vec![(id.clone(), Outcome::NoSpeech)]
    );
    assert_eq!(rig.calls().len(), 1);
    assert_eq!(rig.row(&id).await.status.as_deref(), Some("no_speech"));
}

#[tokio::test]
async fn failures_are_retried_until_max_attempts_then_error() {
    let rig = Rig::new().await;
    let id = rig
        .insert(&rig.write_audio(&tone(2000)), 0, Some("failed"))
        .await;
    let mut settings = rig.settings(&rig.fake("exit 1"));
    settings.grace = Duration::ZERO; // retry as soon as the claim second has passed
    let worker = Worker::new(rig.db.clone(), settings);
    let cancel = CancellationToken::new();

    for (attempt, status, outcome) in [
        (1, "failed", Outcome::Failed),
        (2, "failed", Outcome::Failed),
        (3, "error", Outcome::Error),
    ] {
        let p = worker
            .run_once(&cancel)
            .await
            .expect("db")
            .expect("claimed");
        assert_eq!((p.id.as_str(), p.outcome), (id.as_str(), outcome));
        let row = rig.row(&id).await;
        assert_eq!(
            (row.status.as_deref(), row.attempts),
            (Some(status), attempt)
        );
        tokio::time::sleep(Duration::from_millis(1100)).await;
    }
    assert_eq!(worker.run_once(&cancel).await.expect("db"), None);
    assert!(rig.work_is_empty());
}

#[tokio::test]
async fn a_missing_file_is_retried_and_an_unsafe_path_is_an_error() {
    let rig = Rig::new().await;
    let missing = rig.insert("ptt/2026/11/nope.opus", 0, Some("failed")).await;
    let parent = rig.insert("../outside.opus", 0, Some("failed")).await;
    let absolute = rig.insert("/etc/passwd", 0, Some("failed")).await;
    let mut settings = rig.settings(&rig.fake(SAYS));
    settings.max_attempts = 2;
    settings.grace = Duration::from_secs(1);
    let worker = Worker::new(rig.db.clone(), settings);
    let cancel = CancellationToken::new();

    let mut outcomes = Vec::new();
    for _ in 0..3 {
        let p = worker
            .run_once(&cancel)
            .await
            .expect("db")
            .expect("claimed");
        outcomes.push((p.id, p.outcome));
    }
    assert_eq!(
        outcomes,
        vec![
            (missing.clone(), Outcome::Failed),
            (parent.clone(), Outcome::Error),
            (absolute.clone(), Outcome::Error),
        ]
    );
    assert_eq!(
        worker.run_once(&cancel).await.expect("db"),
        None,
        "retry waits one grace period"
    );
    tokio::time::sleep(Duration::from_millis(2100)).await;
    let p = worker.run_once(&cancel).await.expect("db").expect("retry");
    assert_eq!((p.id, p.outcome), (missing, Outcome::Error));
    assert!(rig.calls().is_empty());
}

#[tokio::test]
async fn timeout_kills_whisper_and_counts_as_a_failure() {
    let rig = Rig::new().await;
    let id = rig
        .insert(&rig.write_audio(&tone(2000)), 0, Some("failed"))
        .await;
    let pid_file = rig.tools.path().join("pid");
    let fake = rig.fake(&format!(
        "echo $$ > '{}'\nexec sleep 30",
        pid_file.display()
    ));
    let mut settings = rig.settings(&fake);
    settings.job_timeout = Duration::from_secs(2);
    let worker = Worker::new(rig.db.clone(), settings);

    let started = Instant::now();
    let p = worker
        .run_once(&CancellationToken::new())
        .await
        .expect("db")
        .expect("claimed");
    assert!(started.elapsed() < Duration::from_secs(5));
    assert_eq!((p.id.as_str(), p.outcome), (id.as_str(), Outcome::Failed));
    assert!(!is_alive(&pid_file), "child killed");
    assert!(rig.work_is_empty());
}

#[tokio::test]
async fn shutdown_kills_whisper_and_releases_the_claim_without_using_an_attempt() {
    let rig = Rig::new().await;
    let id = rig
        .insert(&rig.write_audio(&tone(2000)), 0, Some("unsupported"))
        .await;
    let pid_file = rig.tools.path().join("pid");
    let worker = rig.worker(&rig.fake(&format!(
        "echo $$ > '{}'\nexec sleep 30",
        pid_file.display()
    )));
    let cancel = CancellationToken::new();
    let trigger = cancel.clone();
    let started = pid_file.clone();
    tokio::spawn(async move {
        while !started.exists() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        trigger.cancel();
    });

    let p = worker
        .run_once(&cancel)
        .await
        .expect("db")
        .expect("claimed");
    assert_eq!(p.outcome, Outcome::Released);
    let row = rig.row(&id).await;
    assert_eq!(
        (row.status.as_deref(), row.attempts, row.claimed),
        (Some("failed"), 0, false)
    );
    // kill_on_drop signals without waiting; give the kernel a moment.
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(!is_alive(&pid_file), "child killed");
    assert!(rig.work_is_empty());
}

#[tokio::test]
async fn device_text_that_arrives_meanwhile_is_never_overwritten() {
    let rig = Rig::new().await;
    let id = rig
        .insert(&rig.write_audio(&tone(2000)), 0, Some("failed"))
        .await;
    let worker = rig.worker(&rig.fake(&format!("sleep 1\n{SAYS}")));
    let (db, row_id) = (rig.db.clone(), id.clone());
    let device = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(300)).await;
        // Same statement as the device PUT endpoint.
        sqlx::query(
            "UPDATE tkpttrecording SET Transcript = 'teks perangkat', TranscriptLocale = 'id_ID', \
             TranscriptAt = NOW(), TranscriptSource = 'device', TranscriptStatus = 'done' \
             WHERE RecordingUID = ? AND DeletedAt IS NULL",
        )
        .bind(row_id)
        .execute(&db)
        .await
        .expect("device update");
    });

    let p = worker
        .run_once(&CancellationToken::new())
        .await
        .expect("db")
        .expect("claimed");
    device.await.expect("device task");
    assert_eq!(p.outcome, Outcome::Superseded);
    let row = rig.row(&id).await;
    assert_eq!(row.transcript.as_deref(), Some("teks perangkat"));
    assert_eq!(row.source.as_deref(), Some("device"));
    assert_eq!(row.status.as_deref(), Some("done"));
    assert_eq!(rig.calls().len(), 1, "whisper did run");
}

#[tokio::test]
async fn racing_workers_never_process_a_row_twice() {
    let rig = Rig::new().await;
    let audio = rig.write_audio(&tone(1200));
    let mut ids = HashSet::new();
    for _ in 0..8 {
        ids.insert(rig.insert(&audio, 0, Some("unsupported")).await);
    }
    let fake = rig.fake(&format!("sleep 0.1\n{SAYS}"));
    let (a, b) = (rig.worker(&fake), rig.worker(&fake));
    let (left, right) = tokio::join!(rig.drain(&a), rig.drain(&b));

    assert!(
        !left.is_empty() && !right.is_empty(),
        "both workers got work"
    );
    let all: Vec<String> = left.into_iter().chain(right).map(|(id, _)| id).collect();
    assert_eq!(all.len(), 8);
    assert_eq!(all.iter().cloned().collect::<HashSet<_>>(), ids);
    assert_eq!(rig.calls().len(), 8);
    for id in &ids {
        let row = rig.row(id).await;
        assert_eq!((row.status.as_deref(), row.attempts), (Some("done"), 1));
    }
}

#[tokio::test]
async fn stale_claims_return_to_the_queue_or_end_as_error() {
    let rig = Rig::new().await;
    let audio = rig.write_audio(&tone(1200));
    let stale = rig.insert(&audio, 900, Some("processing")).await;
    let stale_last = rig.insert(&audio, 900, Some("processing")).await;
    let fresh = rig.insert(&audio, 900, Some("processing")).await;
    for (id, sql) in [
        (
            &stale,
            "TranscriptAttempts = 1, TranscriptClaimedAt = NOW() - INTERVAL 600 SECOND",
        ),
        (
            &stale_last,
            "TranscriptAttempts = 3, TranscriptClaimedAt = NOW() - INTERVAL 600 SECOND",
        ),
        (
            &fresh,
            "TranscriptAttempts = 1, TranscriptClaimedAt = NOW() - INTERVAL 10 SECOND",
        ),
    ] {
        rig.exec(
            &format!("UPDATE tkpttrecording SET {sql} WHERE RecordingUID = ?"),
            id,
        )
        .await;
    }
    let worker = rig.worker(&rig.fake(SAYS));

    assert_eq!(worker.recover_stale().await.expect("recover"), 2);
    assert_eq!(rig.row(&stale).await.status.as_deref(), Some("failed"));
    assert_eq!(rig.row(&stale_last).await.status.as_deref(), Some("error"));
    assert_eq!(rig.row(&fresh).await.status.as_deref(), Some("processing"));

    assert_eq!(
        rig.drain(&worker).await,
        vec![(stale.clone(), Outcome::Done)]
    );
    assert_eq!(rig.row(&stale).await.attempts, 2);
}

#[tokio::test]
async fn run_stops_on_cancel() {
    let rig = Rig::new().await;
    let id = rig
        .insert(&rig.write_audio(&tone(1200)), 0, Some("failed"))
        .await;
    let worker = rig.worker(&rig.fake(SAYS));
    let cancel = CancellationToken::new();
    let trigger = cancel.clone();
    let stopper = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(1500)).await;
        trigger.cancel();
    });
    tokio::time::timeout(Duration::from_secs(10), worker.run(cancel))
        .await
        .expect("run returns after cancel");
    stopper.await.expect("stopper");
    assert_eq!(rig.row(&id).await.status.as_deref(), Some("done"));
}

fn is_alive(pid_file: &Path) -> bool {
    let pid = std::fs::read_to_string(pid_file).expect("pid file");
    std::process::Command::new("kill")
        .args(["-0", pid.trim()])
        .stderr(std::process::Stdio::null())
        .status()
        .expect("kill -0")
        .success()
}
