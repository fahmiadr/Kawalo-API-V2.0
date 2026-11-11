//! SQL for `tkpttrecording`. Reads are scoped to the caller's `GroupUID` and `DeletedAt IS NULL`.

use chrono::{DateTime, Utc};
use sqlx::{FromRow, MySqlPool};

#[derive(Debug, FromRow)]
pub struct RecordingRow {
    #[sqlx(rename = "RecordingUID")]
    pub id: String,
    #[sqlx(rename = "SpeakerUserId")]
    pub speaker_user_id: String,
    #[sqlx(rename = "SpeakerEmail")]
    pub speaker_email: Option<String>,
    #[sqlx(rename = "SpeakerName")]
    pub speaker_name: String,
    #[sqlx(rename = "SpeakerRole")]
    pub speaker_role: String,
    #[sqlx(rename = "StartedAt")]
    pub started_at: DateTime<Utc>,
    #[sqlx(rename = "DurationMs")]
    pub duration_ms: i32,
    #[sqlx(rename = "SizeBytes")]
    pub size_bytes: i32,
    #[sqlx(rename = "FilePath")]
    pub file_path: String,
    #[sqlx(rename = "Transcript")]
    pub transcript: Option<String>,
    #[sqlx(rename = "TranscriptLocale")]
    pub transcript_locale: Option<String>,
    #[sqlx(rename = "TranscriptSource")]
    pub transcript_source: Option<String>,
    #[sqlx(rename = "TranscriptStatus")]
    pub transcript_status: Option<String>,
}

macro_rules! columns {
    () => {
        "RecordingUID, SpeakerUserId, SpeakerEmail, SpeakerName, SpeakerRole, StartedAt, \
         DurationMs, SizeBytes, FilePath, Transcript, TranscriptLocale, TranscriptSource, \
         TranscriptStatus"
    };
}

#[derive(Debug)]
pub struct NewRecording<'a> {
    pub id: String,
    pub group_uid: &'a str,
    pub speaker_user_id: &'a str,
    pub speaker_email: Option<&'a str>,
    pub speaker_name: &'a str,
    pub speaker_role: &'a str,
    pub request_id: Option<&'a str>,
    pub started_at: DateTime<Utc>,
    pub duration_ms: i32,
    pub size_bytes: i32,
    pub file_path: &'a str,
}

pub async fn insert(db: &MySqlPool, new: &NewRecording<'_>) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO tkpttrecording (RecordingUID, GroupUID, SpeakerUserId, SpeakerEmail, \
         SpeakerName, SpeakerRole, RequestId, StartedAt, DurationMs, SizeBytes, FilePath) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&new.id)
    .bind(new.group_uid)
    .bind(new.speaker_user_id)
    .bind(new.speaker_email)
    .bind(new.speaker_name)
    .bind(new.speaker_role)
    .bind(new.request_id)
    .bind(new.started_at)
    .bind(new.duration_ms)
    .bind(new.size_bytes)
    .bind(new.file_path)
    .execute(db)
    .await?;
    Ok(())
}

/// Newest first. `before` is the `(StartedAt, RecordingUID)` of the last row already seen.
pub async fn list(
    db: &MySqlPool,
    group_uid: &str,
    before: Option<&(DateTime<Utc>, String)>,
    take: u32,
) -> sqlx::Result<Vec<RecordingRow>> {
    macro_rules! page {
        ($filter:literal) => {
            concat!(
                "SELECT ",
                columns!(),
                " FROM tkpttrecording WHERE GroupUID = ? AND DeletedAt IS NULL",
                $filter,
                " ORDER BY StartedAt DESC, RecordingUID DESC LIMIT ?"
            )
        };
    }
    match before {
        None => {
            sqlx::query_as(page!(""))
                .bind(group_uid)
                .bind(take)
                .fetch_all(db)
                .await
        }
        Some((at, id)) => {
            sqlx::query_as(page!(
                " AND (StartedAt < ? OR (StartedAt = ? AND RecordingUID < ?))"
            ))
            .bind(group_uid)
            .bind(at)
            .bind(at)
            .bind(id)
            .bind(take)
            .fetch_all(db)
            .await
        }
    }
}

pub async fn find(db: &MySqlPool, id: &str, group_uid: &str) -> sqlx::Result<Option<RecordingRow>> {
    sqlx::query_as(concat!(
        "SELECT ",
        columns!(),
        " FROM tkpttrecording WHERE RecordingUID = ? AND GroupUID = ? AND DeletedAt IS NULL"
    ))
    .bind(id)
    .bind(group_uid)
    .fetch_optional(db)
    .await
}

pub async fn soft_delete(db: &MySqlPool, id: &str, now: DateTime<Utc>) -> sqlx::Result<()> {
    sqlx::query("UPDATE tkpttrecording SET DeletedAt = ? WHERE RecordingUID = ?")
        .bind(now)
        .bind(id)
        .execute(db)
        .await?;
    Ok(())
}

/// The id of the speaker's newest live recording for a request id (ids are only unique-ish).
pub async fn find_by_request(
    db: &MySqlPool,
    group_uid: &str,
    speaker_user_id: &str,
    request_id: &str,
) -> sqlx::Result<Option<String>> {
    sqlx::query_scalar(
        "SELECT RecordingUID FROM tkpttrecording WHERE GroupUID = ? AND SpeakerUserId = ? \
         AND RequestId = ? AND DeletedAt IS NULL ORDER BY StartedAt DESC, RecordingUID DESC LIMIT 1",
    )
    .bind(group_uid)
    .bind(speaker_user_id)
    .bind(request_id)
    .fetch_optional(db)
    .await
}

/// Device text always wins over whatever is there, including a server transcript.
pub async fn set_device_transcript(
    db: &MySqlPool,
    id: &str,
    text: &str,
    locale: Option<&str>,
    now: DateTime<Utc>,
) -> sqlx::Result<()> {
    sqlx::query(
        "UPDATE tkpttrecording SET Transcript = ?, TranscriptLocale = ?, TranscriptAt = ?, \
         TranscriptSource = 'device', TranscriptStatus = 'done' \
         WHERE RecordingUID = ? AND DeletedAt IS NULL",
    )
    .bind(text)
    .bind(locale)
    .bind(now)
    .bind(id)
    .execute(db)
    .await?;
    Ok(())
}

/// Records that the device could not transcribe, only while nobody has produced anything: never
/// over text, `processing`, `done`, `no_speech` or `error`.
pub async fn set_device_status(db: &MySqlPool, id: &str, status: &str) -> sqlx::Result<()> {
    sqlx::query(
        "UPDATE tkpttrecording SET TranscriptStatus = ?, TranscriptAttempts = 0 \
         WHERE RecordingUID = ? AND DeletedAt IS NULL AND Transcript IS NULL \
         AND (TranscriptStatus IS NULL OR TranscriptStatus IN ('unsupported', 'failed'))",
    )
    .bind(status)
    .bind(id)
    .execute(db)
    .await?;
    Ok(())
}

pub async fn find_status(db: &MySqlPool, id: &str) -> sqlx::Result<Option<String>> {
    sqlx::query_scalar("SELECT TranscriptStatus FROM tkpttrecording WHERE RecordingUID = ?")
        .bind(id)
        .fetch_one(db)
        .await
}
