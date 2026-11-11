//! Records one speaker turn per floor grant, off the relay path.
//!
//! `Session::on_binary` hands each accepted audio frame to a bounded channel with `try_send`; a
//! task per turn encodes it and, when the channel closes (floor released, revoked, preempted,
//! time-out, socket gone) or the turn is full, writes the Ogg/Opus file and the database row.
//! Nothing here can block or fail the relay: a full channel drops the frame and a failure is
//! logged and forgotten.

use std::fs;
use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use bytes::Bytes;
use chrono::{DateTime, Datelike, SubsecRound, Utc};
use sqlx::MySqlPool;
use tokio::sync::mpsc;
use uuid::Uuid;

use super::recording::{Config, Error, Push, TurnEncoder, TurnFrame};
use super::recordings::repository::{self, NewRecording};
use crate::core::ids::generate_prefixed_uuid_v7;

/// About five seconds of audio. The task encodes far faster than real time, so a full channel
/// means it is stuck, and dropping is better than growing without bound.
const CHANNEL_FRAMES: usize = 256;

/// Largest Opus packet libopus can emit; anything bigger is not audio we can mux.
const MAX_PAYLOAD_BYTES: usize = 1275;

const HEADER_BYTES: usize = 7;
const AUDIO_OPCODE: u8 = 0x10;

/// Who spoke, captured when the floor is granted.
#[derive(Debug, Clone)]
pub struct TurnMeta {
    pub group: Arc<str>,
    pub user_id: String,
    pub email: Option<String>,
    pub name: String,
    pub role: String,
}

#[derive(Debug)]
pub(super) struct Item {
    sequence: u16,
    offset_ms: u32,
    payload: Bytes,
}

/// The sending side of one turn, kept in the room's floor state. Dropping the last clone ends
/// the turn.
#[derive(Debug, Clone)]
pub struct TurnTx {
    tx: mpsc::Sender<Item>,
    dropped: Arc<AtomicU32>,
}

impl TurnTx {
    /// Queues the payload of a `0x10` audio frame (7-byte header: opcode, u16 sequence, u32
    /// offset, big endian). Never blocks; frames that do not fit the channel are counted.
    pub fn offer(&self, frame: &Bytes) {
        let Some(header) = frame.first_chunk::<HEADER_BYTES>() else {
            return;
        };
        if header[0] != AUDIO_OPCODE
            || frame.len() == HEADER_BYTES
            || frame.len() - HEADER_BYTES > MAX_PAYLOAD_BYTES
        {
            return;
        }
        let item = Item {
            sequence: u16::from_be_bytes([header[1], header[2]]),
            offset_ms: u32::from_be_bytes([header[3], header[4], header[5], header[6]]),
            payload: frame.slice(HEADER_BYTES..),
        };
        if let Err(mpsc::error::TrySendError::Full(_)) = self.tx.try_send(item) {
            self.dropped.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// A turn not backed by a task; the receiver observes frames and the end of the turn.
    #[cfg(test)]
    pub(super) fn detached(capacity: usize) -> (Self, mpsc::Receiver<Item>) {
        let (tx, rx) = mpsc::channel(capacity);
        let turn = Self {
            tx,
            dropped: Arc::new(AtomicU32::new(0)),
        };
        (turn, rx)
    }

    #[cfg(test)]
    pub fn dropped(&self) -> u32 {
        self.dropped.load(Ordering::Relaxed)
    }
}

/// Starts turn recorders; one per hub.
#[derive(Debug, Clone)]
pub struct Recorder {
    db: MySqlPool,
    root: Arc<Path>,
}

impl Recorder {
    pub fn new(db: MySqlPool, upload_root: PathBuf) -> Self {
        Self {
            db,
            root: upload_root.into(),
        }
    }

    /// Spawns the task for a turn that starts now.
    pub fn start_turn(&self, meta: TurnMeta) -> TurnTx {
        let (tx, rx) = mpsc::channel(CHANNEL_FRAMES);
        let dropped = Arc::new(AtomicU32::new(0));
        let this = self.clone();
        let counter = Arc::clone(&dropped);
        let started_at = Utc::now().trunc_subsecs(3);
        tokio::spawn(async move {
            // The inner task is the panic boundary: libopus wrappers panic on misuse, and that
            // must end this turn only.
            let user = meta.user_id.clone();
            let job = tokio::spawn(this.run(meta, started_at, rx, counter));
            if let Err(err) = job.await {
                tracing::warn!(error = %err, "[PTT] recorder task for user {user} failed");
            }
        });
        TurnTx { tx, dropped }
    }

    async fn run(
        self,
        meta: TurnMeta,
        started_at: DateTime<Utc>,
        mut rx: mpsc::Receiver<Item>,
        dropped: Arc<AtomicU32>,
    ) {
        let serial = Uuid::new_v4().as_fields().0;
        let mut encoder = match TurnEncoder::new(Vec::new(), serial, Config::default()) {
            Ok(encoder) => encoder,
            Err(err) => {
                tracing::warn!(error = %err, "[PTT] could not start a recording encoder");
                return;
            }
        };
        // ponytail: encoding runs inline, about 0.1 ms per 20 ms frame; move it to
        // spawn_blocking if the per-frame cost ever matters on the async workers.
        while let Some(item) = rx.recv().await {
            let frame = TurnFrame {
                sequence: item.sequence,
                offset_ms: item.offset_ms,
                payload: &item.payload,
            };
            match encoder.push(frame) {
                Ok(Push::Full) => break,
                Ok(_) => {}
                Err(err) => {
                    tracing::warn!(error = %err, "[PTT] recording aborted while encoding");
                    return;
                }
            }
        }
        // Closing the receiver makes later offers no-ops once the turn is full.
        drop(rx);
        let lost = dropped.load(Ordering::Relaxed);
        if lost > 0 {
            tracing::warn!(
                "[PTT] recorder queue was full: {lost} frames of user {} were not recorded",
                meta.user_id
            );
        }

        let duration_ms = encoder.duration_ms();
        let root = Arc::clone(&self.root);
        let saved = tokio::task::spawn_blocking(move || {
            let bytes = encoder.finish()?;
            let size = bytes.len();
            let relative = store(&root, started_at, &bytes)?;
            Ok::<_, StoreError>((relative, size))
        })
        .await;
        let (relative, size) = match saved {
            Ok(Ok(saved)) => saved,
            Ok(Err(StoreError::Encode(Error::Empty))) => return,
            Ok(Err(err)) => {
                tracing::warn!(error = %err, "[PTT] could not save a recording");
                return;
            }
            Err(err) => {
                tracing::warn!(error = %err, "[PTT] recording writer failed");
                return;
            }
        };

        let new = NewRecording {
            id: generate_prefixed_uuid_v7("rec"),
            group_uid: &meta.group,
            speaker_user_id: &meta.user_id,
            speaker_email: meta.email.as_deref(),
            speaker_name: &meta.name,
            speaker_role: &meta.role,
            started_at,
            duration_ms: i32::try_from(duration_ms).unwrap_or(i32::MAX),
            size_bytes: i32::try_from(size).unwrap_or(i32::MAX),
            file_path: &relative,
        };
        if let Err(err) = repository::insert(&self.db, &new).await {
            tracing::warn!(error = %err, "[PTT] could not insert a recording row");
            let root = Arc::clone(&self.root);
            let _ = tokio::task::spawn_blocking(move || fs::remove_file(root.join(relative))).await;
        }
    }
}

#[derive(Debug, thiserror::Error)]
enum StoreError {
    #[error(transparent)]
    Encode(#[from] Error),
    #[error("writing the file failed: {0}")]
    Io(#[from] std::io::Error),
}

/// Writes `ptt/{yyyy}/{mm}/{uuid}.opus` under `root` through a temporary file in the same
/// directory so readers never see a partial file. Returns the path relative to `root`.
fn store(root: &Path, at: DateTime<Utc>, data: &[u8]) -> std::io::Result<String> {
    let (year, month) = (format!("{:04}", at.year()), format!("{:02}", at.month()));
    let dir = root.join("ptt").join(&year).join(&month);
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o755)
        .create(&dir)?;
    let id = Uuid::new_v4();
    let name = format!("{id}.opus");
    let tmp = dir.join(format!(".{id}.tmp"));
    let result = (|| {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o644)
            .open(&tmp)?;
        file.write_all(data)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&tmp, dir.join(&name))
    })();
    if let Err(err) = result {
        let _ = fs::remove_file(&tmp);
        return Err(err);
    }
    Ok(format!("ptt/{year}/{month}/{name}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(sequence: u16, offset: u32, payload: &[u8]) -> Bytes {
        let mut out = vec![AUDIO_OPCODE];
        out.extend_from_slice(&sequence.to_be_bytes());
        out.extend_from_slice(&offset.to_be_bytes());
        out.extend_from_slice(payload);
        Bytes::from(out)
    }

    #[test]
    fn a_full_channel_drops_frames_instead_of_blocking() {
        let (tx, mut rx) = mpsc::channel(2);
        let turn = TurnTx {
            tx,
            dropped: Arc::new(AtomicU32::new(0)),
        };
        for n in 0..5 {
            turn.offer(&frame(n, u32::from(n) * 20, &[0; 640]));
        }
        assert_eq!(turn.dropped(), 3);
        let first = rx.try_recv().expect("first frame kept");
        assert_eq!(
            (first.sequence, first.offset_ms, first.payload.len()),
            (0, 0, 640)
        );
        assert_eq!(rx.try_recv().expect("second").sequence, 1);
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn only_well_formed_audio_frames_are_queued() {
        let (tx, mut rx) = mpsc::channel(8);
        let turn = TurnTx {
            tx,
            dropped: Arc::new(AtomicU32::new(0)),
        };
        turn.offer(&Bytes::from_static(&[AUDIO_OPCODE, 0, 0]));
        turn.offer(&frame(0, 0, &[]));
        turn.offer(&frame(0, 0, &[0; MAX_PAYLOAD_BYTES + 1]));
        let mut other = frame(0, 0, &[0; 640]).to_vec();
        other[0] = 0x11;
        turn.offer(&Bytes::from(other));
        assert!(rx.try_recv().is_err());
        assert_eq!(turn.dropped(), 0);
        turn.offer(&frame(7, 140, &[1; 10]));
        let item = rx.try_recv().expect("valid frame");
        assert_eq!((item.sequence, item.offset_ms), (7, 140));
    }

    #[test]
    fn offering_to_a_finished_turn_is_a_silent_no_op() {
        let (tx, rx) = mpsc::channel(1);
        let turn = TurnTx {
            tx,
            dropped: Arc::new(AtomicU32::new(0)),
        };
        drop(rx);
        turn.offer(&frame(0, 0, &[0; 640]));
        assert_eq!(turn.dropped(), 0);
    }

    #[test]
    fn store_writes_under_the_ptt_directory_and_leaves_no_temp_file() {
        let root = tempfile::tempdir().expect("tmp");
        let at = DateTime::parse_from_rfc3339("2026-11-11T10:00:00Z")
            .expect("date")
            .with_timezone(&Utc);
        let relative = store(root.path(), at, b"OggS").expect("stored");
        assert!(relative.starts_with("ptt/2026/11/") && relative.ends_with(".opus"));
        assert_eq!(
            fs::read(root.path().join(&relative)).expect("read"),
            b"OggS"
        );
        let dir = root.path().join("ptt/2026/11");
        assert_eq!(fs::read_dir(dir).expect("dir").count(), 1);
    }
}
