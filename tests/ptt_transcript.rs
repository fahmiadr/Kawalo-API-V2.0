//! Client-side PTT transcripts: the request id of a WebSocket turn lands on its recording, and
//! `PUT /api/v1/ptt/recordings/by-request/{request_id}/transcript` attaches the speaker's device
//! text. Needs the scratch services from `scripts/dev-services.sh start` (including the
//! `20261111020000_add_ptt_transcript` migration).

mod common;
mod ptt_support;

use std::time::Duration;

use axum::http::StatusCode;
use chrono::{Duration as Span, SubsecRound, Utc};
use common::TestApp;
use ptt_support::*;
use serde_json::{Value, json};
use tokio_tungstenite::tungstenite::Message;
use uuid::Uuid;

struct Rig {
    app: TestApp,
    _dir: tempfile::TempDir,
    squad: Squad,
}

impl Rig {
    async fn new() -> Self {
        let dir = tempfile::tempdir().expect("upload dir");
        let root = dir.path().to_owned();
        let app = TestApp::start_with(move |config| config.upload_dir = root).await;
        Self {
            app,
            _dir: dir,
            squad: Squad::new(),
        }
    }

    fn group(&self) -> &str {
        &self.squad.group
    }

    fn email(&self, who: &str) -> String {
        format!("{}@kawalo.test", self.squad.user(who))
    }

    async fn guard(&self, who: &str) -> String {
        self.squad.token(&self.app, who, false).await
    }

    async fn put(
        &self,
        token: Option<&str>,
        request_id: &str,
        body: &Value,
    ) -> (StatusCode, Value) {
        self.app
            .call(
                "PUT",
                &format!("/api/v1/ptt/recordings/by-request/{request_id}/transcript"),
                token,
                Some(body),
            )
            .await
    }

    async fn list(&self, token: &str) -> Vec<Value> {
        let (status, json) = self
            .app
            .call("GET", "/api/v1/ptt/recordings", Some(token), None)
            .await;
        assert_eq!(status, StatusCode::OK, "{json}");
        json["data"]["items"].as_array().expect("items").clone()
    }

    /// A recording as the recorder would have written it, `age_s` seconds ago.
    async fn seed(&self, group: &str, id: &str, speaker: &str, request_id: &str, age_s: i64) {
        sqlx::query(
            "INSERT INTO tkpttrecording (RecordingUID, GroupUID, SpeakerUserId, SpeakerEmail, \
             SpeakerName, SpeakerRole, RequestId, StartedAt, DurationMs, SizeBytes, FilePath) \
             VALUES (?, ?, ?, ?, 'U', 'GUARD', ?, ?, 1000, 1000, 'ptt/x.opus')",
        )
        .bind(id)
        .bind(group)
        .bind(speaker)
        .bind(speaker)
        .bind(request_id)
        .bind((Utc::now() - Span::seconds(age_s)).trunc_subsecs(0))
        .execute(&self.app.state.db)
        .await
        .expect("seed row");
    }

    async fn sql(&self, statement: &'static str, id: &str) {
        sqlx::query(statement)
            .bind(id)
            .execute(&self.app.state.db)
            .await
            .expect("sql");
    }

    async fn status_of(&self, id: &str) -> Option<String> {
        sqlx::query_scalar("SELECT TranscriptStatus FROM tkpttrecording WHERE RecordingUID = ?")
            .bind(id)
            .fetch_one(&self.app.state.db)
            .await
            .expect("row")
    }
}

fn new_id() -> String {
    format!("rec_{}", Uuid::new_v4().simple())
}

fn pcm_frame(sequence: u16) -> Vec<u8> {
    let mut frame = vec![0x10];
    frame.extend_from_slice(&sequence.to_be_bytes());
    frame.extend_from_slice(&(u32::from(sequence) * 20).to_be_bytes());
    for i in 0..320 {
        let t = (usize::from(sequence) * 320 + i) as f32 / 16_000.0;
        let sample = (8_000.0 * (2.0 * std::f32::consts::PI * 440.0 * t).sin()) as i16;
        frame.extend_from_slice(&sample.to_le_bytes());
    }
    frame
}

/// One full turn over a real socket with the given `requestId` value (`None` omits it); returns
/// the `RequestId` stored with the resulting recording.
async fn turn_stores(request_id: Option<Value>) -> Option<String> {
    let rig = Rig::new().await;
    let token = rig.guard("budi").await;
    let addr = rig.app.serve().await;
    let mut ws = join(addr, rig.group(), &token).await;
    ws.json().await;
    let mut frame = json!({"type": "FLOOR_REQUEST"});
    if let Some(id) = request_id {
        frame["requestId"] = id;
    }
    ws.send(&frame).await;
    loop {
        if let Message::Text(text) = ws.next_message().await
            && text.contains("FLOOR_GRANTED")
        {
            break;
        }
    }
    for sequence in 0..20 {
        ws.audio(&pcm_frame(sequence)).await;
    }
    ws.send(&json!({"type": "FLOOR_RELEASED"})).await;
    for _ in 0..60 {
        let rows: Vec<Option<String>> =
            sqlx::query_scalar("SELECT RequestId FROM tkpttrecording WHERE GroupUID = ?")
                .bind(rig.group())
                .fetch_all(&rig.app.state.db)
                .await
                .expect("rows");
        if let [one] = rows.as_slice() {
            return one.clone();
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("the recording was not written in 6s");
}

#[tokio::test]
async fn a_string_request_id_is_stored_with_the_recording() {
    assert_eq!(
        turn_stores(Some(json!("req-123"))).await.as_deref(),
        Some("req-123")
    );
    let longest = "x".repeat(64);
    assert_eq!(
        turn_stores(Some(json!(longest))).await.as_deref(),
        Some(longest.as_str())
    );
}

#[tokio::test]
async fn other_request_ids_store_null() {
    for id in [
        None,
        Some(json!(5)),
        Some(json!(null)),
        Some(json!({"a": 1})),
        Some(json!("")),
        Some(json!("x".repeat(65))),
    ] {
        assert_eq!(turn_stores(id.clone()).await, None, "{id:?}");
    }
}

#[tokio::test]
async fn the_speaker_attaches_device_text_and_the_list_shows_it() {
    let rig = Rig::new().await;
    let (budi, ani) = (rig.guard("budi").await, rig.guard("ani").await);
    let id = new_id();
    rig.seed(rig.group(), &id, &rig.email("budi"), "r1", 5)
        .await;

    let body =
        json!({"status": "ok", "text": "  siap\u{0}\u{7} laksanakan\n  ", "locale": "id_ID"});
    let (status, json) = rig.put(Some(&budi), "r1", &body).await;
    assert_eq!(status, StatusCode::OK, "{json}");
    assert_eq!(json["success"], true);
    assert_eq!(json["message"], "Transkrip berhasil disimpan");
    assert_eq!(json["data"], json!({"id": id, "transcript_status": "done"}));

    // Everyone in the group sees it.
    for token in [&budi, &ani] {
        let items = rig.list(token).await;
        assert_eq!(items.len(), 1);
        assert_eq!(items[0]["transcript"], "siap laksanakan");
        assert_eq!(items[0]["transcript_locale"], "id_ID");
        assert_eq!(items[0]["transcript_source"], "device");
        assert_eq!(items[0]["transcript_status"], "done");
    }
}

#[tokio::test]
async fn a_list_item_without_a_transcript_has_nulls() {
    let rig = Rig::new().await;
    let budi = rig.guard("budi").await;
    rig.seed(rig.group(), &new_id(), &rig.email("budi"), "r1", 5)
        .await;
    let items = rig.list(&budi).await;
    for key in [
        "transcript",
        "transcript_locale",
        "transcript_source",
        "transcript_status",
    ] {
        assert_eq!(items[0][key], Value::Null, "{key}");
    }
}

#[tokio::test]
async fn only_the_speaker_of_that_group_may_write() {
    let rig = Rig::new().await;
    let (budi, ani) = (rig.guard("budi").await, rig.guard("ani").await);
    let id = new_id();
    rig.seed(rig.group(), &id, &rig.email("budi"), "r1", 5)
        .await;
    let body = json!({"status": "ok", "text": "halo"});

    let (status, json) = rig.put(Some(&ani), "r1", &body).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{json}");
    assert_eq!(json["message"], "Rekaman tidak ditemukan.");

    // Same user id in another group.
    let elsewhere = Squad::new();
    let outsider = rig
        .app
        .token_for(&rig.email("budi"), &elsewhere.group, false)
        .await;
    let (status, _) = rig.put(Some(&outsider), "r1", &body).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, _) = rig.put(Some(&budi), "unknown", &body).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(rig.status_of(&id).await, None);
}

#[tokio::test]
async fn writing_needs_a_token() {
    let rig = Rig::new().await;
    let (status, json) = rig
        .put(None, "r1", &json!({"status": "ok", "text": "halo"}))
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{json}");
}

#[tokio::test]
async fn bad_bodies_are_422() {
    let rig = Rig::new().await;
    let budi = rig.guard("budi").await;
    let id = new_id();
    rig.seed(rig.group(), &id, &rig.email("budi"), "r1", 5)
        .await;
    for body in [
        json!({"status": "ok"}),
        json!({"status": "ok", "text": ""}),
        json!({"status": "ok", "text": "   \n"}),
        json!({"status": "ok", "text": "\u{0}\u{1}"}),
        json!({"status": "ok", "text": "a".repeat(2001)}),
        json!({"status": "ok", "text": "a", "locale": "x"}),
        json!({"status": "ok", "text": "a", "locale": "x".repeat(17)}),
        json!({"status": "unsupported", "text": "halo"}),
        json!({"status": "failed", "text": "halo"}),
        json!({"status": "done", "text": "halo"}),
        json!({"text": "halo"}),
    ] {
        let (status, json) = rig.put(Some(&budi), "r1", &body).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}: {json}");
        assert_eq!(json["error_code"], "VALIDATION_ERROR", "{body}");
    }
    assert_eq!(rig.status_of(&id).await, None);
    // The limits themselves pass.
    let body = json!({"status": "ok", "text": "a".repeat(2000), "locale": "id"});
    assert_eq!(rig.put(Some(&budi), "r1", &body).await.0, StatusCode::OK);
}

#[tokio::test]
async fn the_same_text_twice_is_idempotent() {
    let rig = Rig::new().await;
    let budi = rig.guard("budi").await;
    rig.seed(rig.group(), &new_id(), &rig.email("budi"), "r1", 5)
        .await;
    let body = json!({"status": "ok", "text": "halo", "locale": "id-ID"});
    let first = rig.put(Some(&budi), "r1", &body).await;
    let second = rig.put(Some(&budi), "r1", &body).await;
    assert_eq!(first.0, StatusCode::OK);
    assert_eq!(first, second);
}

#[tokio::test]
async fn device_text_overwrites_server_and_earlier_device_text() {
    let rig = Rig::new().await;
    let budi = rig.guard("budi").await;
    let id = new_id();
    rig.seed(rig.group(), &id, &rig.email("budi"), "r1", 5)
        .await;
    rig.sql(
        "UPDATE tkpttrecording SET Transcript = 'x', TranscriptSource = 'server', \
         TranscriptStatus = 'done', TranscriptLocale = 'id' WHERE RecordingUID = ?",
        &id,
    )
    .await;

    let (status, _) = rig
        .put(
            Some(&budi),
            "r1",
            &json!({"status": "ok", "text": "dari perangkat"}),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let item = rig.list(&budi).await.remove(0);
    assert_eq!(item["transcript"], "dari perangkat");
    assert_eq!(item["transcript_source"], "device");
    // No locale sent: the old one does not linger next to new text.
    assert_eq!(item["transcript_locale"], Value::Null);

    let (status, _) = rig
        .put(
            Some(&budi),
            "r1",
            &json!({"status": "ok", "text": "koreksi"}),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(rig.list(&budi).await[0]["transcript"], "koreksi");

    // Device text also beats a row a worker has claimed.
    rig.sql("UPDATE tkpttrecording SET Transcript = NULL, TranscriptStatus = 'processing' WHERE RecordingUID = ?", &id).await;
    let (status, json) = rig
        .put(
            Some(&budi),
            "r1",
            &json!({"status": "ok", "text": "menang"}),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["data"]["transcript_status"], "done");
}

#[tokio::test]
async fn failure_reports_fill_only_an_empty_status() {
    let rig = Rig::new().await;
    let budi = rig.guard("budi").await;
    let id = new_id();
    rig.seed(rig.group(), &id, &rig.email("budi"), "r1", 5)
        .await;

    let (status, json) = rig
        .put(Some(&budi), "r1", &json!({"status": "unsupported"}))
        .await;
    assert_eq!(status, StatusCode::OK, "{json}");
    assert_eq!(
        json["data"],
        json!({"id": id, "transcript_status": "unsupported"})
    );
    // Empty text counts as absent; a later report may change the label.
    let (_, json) = rig
        .put(Some(&budi), "r1", &json!({"status": "failed", "text": " "}))
        .await;
    assert_eq!(json["data"]["transcript_status"], "failed");

    // Attempts restart when the status is set, so the worker has its full budget.
    rig.sql(
        "UPDATE tkpttrecording SET TranscriptAttempts = 2 WHERE RecordingUID = ?",
        &id,
    )
    .await;
    rig.put(Some(&budi), "r1", &json!({"status": "unsupported"}))
        .await;
    let attempts: u8 =
        sqlx::query_scalar("SELECT TranscriptAttempts FROM tkpttrecording WHERE RecordingUID = ?")
            .bind(&id)
            .fetch_one(&rig.app.state.db)
            .await
            .expect("attempts");
    assert_eq!(attempts, 0);

    for (state, attempts) in [
        ("processing", 1),
        ("done", 0),
        ("no_speech", 1),
        ("error", 3),
    ] {
        sqlx::query("UPDATE tkpttrecording SET TranscriptStatus = ?, TranscriptAttempts = ? WHERE RecordingUID = ?")
            .bind(state)
            .bind(attempts)
            .bind(&id)
            .execute(&rig.app.state.db)
            .await
            .expect("state");
        let (status, json) = rig
            .put(Some(&budi), "r1", &json!({"status": "failed"}))
            .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(json["data"]["transcript_status"], state);
        let kept: u8 = sqlx::query_scalar(
            "SELECT TranscriptAttempts FROM tkpttrecording WHERE RecordingUID = ?",
        )
        .bind(&id)
        .fetch_one(&rig.app.state.db)
        .await
        .expect("attempts");
        assert_eq!(kept, attempts, "{state}");
    }

    // A row that already holds text keeps its status whatever the label.
    sqlx::query("UPDATE tkpttrecording SET Transcript = 't', TranscriptStatus = NULL WHERE RecordingUID = ?")
        .bind(&id)
        .execute(&rig.app.state.db)
        .await
        .expect("text");
    let (_, json) = rig
        .put(Some(&budi), "r1", &json!({"status": "unsupported"}))
        .await;
    assert_eq!(json["data"]["transcript_status"], Value::Null);
}

#[tokio::test]
async fn the_newest_recording_of_a_reused_request_id_gets_the_text() {
    let rig = Rig::new().await;
    let budi = rig.guard("budi").await;
    let (old, new) = (new_id(), new_id());
    rig.seed(rig.group(), &old, &rig.email("budi"), "r1", 60)
        .await;
    rig.seed(rig.group(), &new, &rig.email("budi"), "r1", 5)
        .await;
    let (_, json) = rig
        .put(Some(&budi), "r1", &json!({"status": "ok", "text": "baru"}))
        .await;
    assert_eq!(json["data"]["id"], json!(new));
    assert_eq!(rig.status_of(&old).await, None);
}

#[tokio::test]
async fn a_deleted_recording_is_hidden_and_refuses_text() {
    let rig = Rig::new().await;
    let (budi, boss) = (
        rig.guard("budi").await,
        rig.squad.token(&rig.app, "boss", true).await,
    );
    let id = new_id();
    rig.seed(rig.group(), &id, &rig.email("budi"), "r1", 5)
        .await;
    let body = json!({"status": "ok", "text": "halo"});
    assert_eq!(rig.put(Some(&budi), "r1", &body).await.0, StatusCode::OK);

    let (status, json) = rig
        .app
        .call(
            "DELETE",
            &format!("/api/v1/ptt/recordings/{id}"),
            Some(&boss),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{json}");
    assert!(rig.list(&budi).await.is_empty());
    let (status, _) = rig.put(Some(&budi), "r1", &body).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
