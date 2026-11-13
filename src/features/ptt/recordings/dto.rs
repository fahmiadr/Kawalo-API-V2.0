//! Query and response shapes for `/ptt/recordings` (snake_case, like the other features).

use base64::Engine;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::repository::RecordingRow;
use crate::core::clean::iso_string;
use crate::core::constants::{DEFAULT_PAGE_SIZE, MAX_PAGE_SIZE, default_api_prefix};
use crate::core::extract::schema::{Body, Field, Kind, Schema};
use crate::core::extract::{Numeric, Validate, ValidationErrors};

#[derive(Debug, Default, Deserialize)]
pub struct ListQuery {
    pub limit: Option<Numeric>,
    pub before: Option<String>,
}

impl ListQuery {
    pub fn take(&self) -> u32 {
        // Validated to 1..=MAX_PAGE_SIZE; fractions truncate like the other lists.
        self.limit
            .and_then(Numeric::value)
            .map_or(DEFAULT_PAGE_SIZE, |n| n as u32)
    }

    pub fn cursor(&self) -> Option<(DateTime<Utc>, String)> {
        self.before.as_deref().and_then(decode_cursor)
    }
}

impl Validate for ListQuery {
    fn validate(&self) -> Result<(), ValidationErrors> {
        let mut errors = ValidationErrors::new();
        if let Some(before) = self.before.as_deref()
            && decode_cursor(before).is_none()
        {
            errors.push("before", "Kursor tidak valid");
        }
        errors.numeric("limit", self.limit, 1.0..=f64::from(MAX_PAGE_SIZE));
        errors.into_result()
    }
}

/// Opaque to clients: `base64url("{started_at_ms}.{recording_id}")`.
pub fn encode_cursor(at: DateTime<Utc>, id: &str) -> String {
    URL_SAFE_NO_PAD.encode(format!("{}.{id}", at.timestamp_millis()))
}

fn decode_cursor(raw: &str) -> Option<(DateTime<Utc>, String)> {
    let bytes = URL_SAFE_NO_PAD.decode(raw).ok()?;
    let text = std::str::from_utf8(&bytes).ok()?;
    let (millis, id) = text.split_once('.')?;
    (!id.is_empty() && id.len() <= 64).then_some(())?;
    let at = DateTime::from_timestamp_millis(millis.parse().ok()?)?;
    Some((at, id.to_owned()))
}

#[derive(Debug, Serialize)]
pub struct Speaker {
    pub id: String,
    pub name: String,
    pub role: String,
    pub email: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct RecordingItem {
    pub id: String,
    pub speaker: Speaker,
    pub started_at: String,
    pub duration_ms: i32,
    pub size_bytes: i32,
    /// Absolute path on this server; needs the same `Authorization` header as the list.
    pub audio_path: String,
    /// Statuses: `done`, `unsupported`, `failed`, `processing`, `no_speech`, `error`, or `null`.
    pub transcript: Option<String>,
    pub transcript_locale: Option<String>,
    pub transcript_source: Option<String>,
    pub transcript_status: Option<String>,
    /// Base64 of `WAVEFORM_BARS` loudness bytes (0..=255) over the turn; `null` for turns
    /// recorded before waveforms existed or holding passthrough Opus.
    pub waveform: Option<String>,
}

impl From<RecordingRow> for RecordingItem {
    fn from(row: RecordingRow) -> Self {
        Self {
            audio_path: format!("{}/ptt/recordings/{}/audio", default_api_prefix(), row.id),
            id: row.id,
            speaker: Speaker {
                id: row.speaker_user_id,
                name: row.speaker_name,
                role: row.speaker_role,
                email: row.speaker_email,
            },
            started_at: iso_string(row.started_at),
            duration_ms: row.duration_ms,
            size_bytes: row.size_bytes,
            transcript: row.transcript,
            transcript_locale: row.transcript_locale,
            transcript_source: row.transcript_source,
            transcript_status: row.transcript_status,
            waveform: row.waveform.map(|bars| STANDARD.encode(bars)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TranscriptState {
    Ok,
    Unsupported,
    Failed,
}

impl TranscriptState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Unsupported => "unsupported",
            Self::Failed => "failed",
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct TranscriptBody {
    pub status: TranscriptState,
    pub text: Option<String>,
    pub locale: Option<String>,
}

impl Body for TranscriptBody {
    const SCHEMA: Schema = Schema::new(&[
        Field::required("status", Kind::OneOf(&["ok", "unsupported", "failed"])),
        Field::nullable("text", Kind::text(0, 2000)),
        Field::nullable("locale", Kind::text(2, 16)),
    ]);
}

#[derive(Debug, Serialize)]
pub struct TranscriptSaved {
    pub id: String,
    pub transcript_status: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct RecordingPage {
    pub items: Vec<RecordingItem>,
    /// Pass as `before` for the next (older) page; `null` on the last page.
    pub next_before: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_round_trips_and_rejects_junk() {
        let at = DateTime::from_timestamp_millis(1_762_850_000_123).expect("date");
        let cursor = encode_cursor(at, "rec_abc");
        assert_eq!(decode_cursor(&cursor), Some((at, "rec_abc".to_owned())));
        for junk in [
            "",
            "!!",
            "YWJj",
            &URL_SAFE_NO_PAD.encode("x.rec"),
            &URL_SAFE_NO_PAD.encode("1."),
        ] {
            assert_eq!(decode_cursor(junk), None, "{junk}");
        }
    }
}
