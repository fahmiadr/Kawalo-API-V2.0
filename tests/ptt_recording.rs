//! Push-to-talk recording: WebSocket turns become Ogg/Opus files and `tkpttrecording` rows, which
//! the `/api/v1/ptt/recordings` endpoints list, stream and delete. Needs the scratch services
//! from `scripts/dev-services.sh start` (including the `20261111010000_add_ptt_recording`
//! migration).

mod common;
mod ptt_support;

use std::io::Cursor;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{HeaderMap, Method, Request, StatusCode, header};
use base64::Engine;
use chrono::{DateTime, SubsecRound, Utc};
use common::TestApp;
use http_body_util::BodyExt;
use ptt_support::*;
use serde_json::{Value, json};
use tempfile::TempDir;
use tokio_tungstenite::tungstenite::Message;
use tower::ServiceExt;
use uuid::Uuid;

struct Rig {
    app: TestApp,
    dir: TempDir,
    squad: Squad,
}

impl Rig {
    async fn new() -> Self {
        let dir = tempfile::tempdir().expect("upload dir");
        let root = dir.path().to_owned();
        let app = TestApp::start_with(move |config| {
            config.upload_dir = root;
        })
        .await;
        Self {
            app,
            dir,
            squad: Squad::new(),
        }
    }

    fn group(&self) -> &str {
        &self.squad.group
    }

    async fn guard(&self, who: &str) -> String {
        self.squad.token(&self.app, who, false).await
    }

    async fn admin(&self, who: &str) -> String {
        self.squad.token(&self.app, who, true).await
    }

    async fn user(&self, who: &str) -> String {
        custom_token(
            &self.app,
            &self.squad.user(who),
            "USER",
            Some(self.group()),
            0,
        )
        .await
    }

    async fn raw(
        &self,
        method: Method,
        path: &str,
        token: Option<&str>,
        range: Option<&str>,
    ) -> (StatusCode, HeaderMap, Vec<u8>) {
        raw_on(&self.app.router, method, path, token, range).await
    }

    async fn list(&self, token: &str, query: &str) -> (StatusCode, Value) {
        self.app
            .call(
                "GET",
                &format!("/api/v1/ptt/recordings{query}"),
                Some(token),
                None,
            )
            .await
    }

    /// Polls the list until it holds `count` items; the recorder finishes after the turn ends.
    async fn wait_for(&self, token: &str, count: usize) -> Vec<Value> {
        for _ in 0..60 {
            let (status, json) = self.list(token, "").await;
            assert_eq!(status, StatusCode::OK, "{json}");
            let items = json["data"]["items"].as_array().expect("items").clone();
            if items.len() >= count {
                return items;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        panic!("recordings did not reach {count} in 6s");
    }

    async fn insert(&self, group: &str, id: &str, started_at: DateTime<Utc>, path: &str) {
        sqlx::query(
            "INSERT INTO tkpttrecording (RecordingUID, GroupUID, SpeakerUserId, SpeakerEmail, \
             SpeakerName, SpeakerRole, StartedAt, DurationMs, SizeBytes, FilePath) \
             VALUES (?, ?, 'u@x.id', 'u@x.id', 'U', 'GUARD', ?, 1000, 1000, ?)",
        )
        .bind(id)
        .bind(group)
        .bind(started_at)
        .bind(path)
        .execute(&self.app.state.db)
        .await
        .expect("insert row");
    }

    fn write(&self, relative: &str, bytes: &[u8]) -> PathBuf {
        let path = self.dir.path().join(relative);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("dirs");
        std::fs::write(&path, bytes).expect("write file");
        path
    }

    async fn deleted_at(&self, id: &str) -> Option<DateTime<Utc>> {
        sqlx::query_scalar("SELECT DeletedAt FROM tkpttrecording WHERE RecordingUID = ?")
            .bind(id)
            .fetch_one(&self.app.state.db)
            .await
            .expect("row stays")
    }
}

async fn raw_on(
    router: &Router,
    method: Method,
    path: &str,
    token: Option<&str>,
    range: Option<&str>,
) -> (StatusCode, HeaderMap, Vec<u8>) {
    let mut builder = Request::builder().method(method).uri(path);
    if let Some(token) = token {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }
    if let Some(range) = range {
        builder = builder.header(header::RANGE, range);
    }
    let mut request = builder.body(Body::empty()).expect("request");
    request.extensions_mut().insert(ConnectInfo(
        "127.0.0.1:50000".parse::<SocketAddr>().expect("peer"),
    ));
    let response = router.clone().oneshot(request).await.expect("infallible");
    let (parts, body) = response.into_parts();
    let bytes = body.collect().await.expect("body").to_bytes().to_vec();
    (parts.status, parts.headers, bytes)
}

/// 20 ms of a 440 Hz tone as 16 kHz mono 16-bit little-endian PCM, behind the 7-byte header.
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

async fn request_floor(ws: &mut Ws) {
    ws.send(&json!({"type": "FLOOR_REQUEST", "requestId": "r"}))
        .await;
    // Other speakers' audio may arrive first when this socket is also a listener.
    loop {
        if let Message::Text(text) = ws.next_message().await
            && text.contains("FLOOR_GRANTED")
        {
            return;
        }
    }
}

async fn send_audio(ws: &mut Ws, frames: u16) {
    for sequence in 0..frames {
        ws.audio(&pcm_frame(sequence)).await;
    }
}

async fn release(ws: &mut Ws) {
    ws.send(&json!({"type": "FLOOR_RELEASED"})).await;
}

struct Decoded {
    packets: usize,
    pre_skip: usize,
    granule: u64,
    pcm: Vec<i16>,
}

/// Parses the Ogg/Opus bytes the way a player would and decodes them at 16 kHz.
fn decode(bytes: &[u8]) -> Decoded {
    use unsafe_libopus::{OPUS_OK, opus_decode, opus_decoder_create, opus_decoder_destroy};

    assert_eq!(&bytes[..4], b"OggS");
    let mut reader = ogg::PacketReader::new(Cursor::new(bytes));
    let head = reader.read_packet().expect("head").expect("packet");
    assert!(head.first_in_stream());
    assert_eq!(&head.data[..8], b"OpusHead");
    let pre_skip = usize::from(u16::from_le_bytes([head.data[10], head.data[11]]));
    let tags = reader.read_packet().expect("tags").expect("packet");
    assert_eq!(&tags.data[..8], b"OpusTags");

    let mut err = OPUS_OK;
    // SAFETY: `err` outlives the call.
    let decoder = unsafe { opus_decoder_create(16_000, 1, &mut err) };
    assert_eq!(err, OPUS_OK);
    let (mut packets, mut granule, mut ended, mut pcm) = (0, 0, false, Vec::new());
    while let Some(packet) = reader.read_packet().expect("packet") {
        packets += 1;
        let mut out = vec![0i16; 5760];
        // SAFETY: pointers and lengths describe `packet.data` and `out`.
        let n = unsafe {
            opus_decode(
                decoder,
                packet.data.as_ptr(),
                packet.data.len() as i32,
                out.as_mut_ptr(),
                out.len() as i32,
                0,
            )
        };
        assert!(n > 0, "libopus rejected a packet: {n}");
        pcm.extend_from_slice(&out[..n as usize]);
        if packet.last_in_page() {
            granule = packet.absgp_page();
        }
        ended = packet.last_in_stream();
    }
    // SAFETY: created above, destroyed once.
    unsafe { opus_decoder_destroy(decoder) };
    assert!(ended, "the last page carries the end-of-stream flag");
    Decoded {
        packets,
        pre_skip,
        granule,
        pcm,
    }
}

fn rms(pcm: &[i16]) -> f64 {
    (pcm.iter().map(|&s| f64::from(s).powi(2)).sum::<f64>() / pcm.len() as f64).sqrt()
}

fn audio_url(item: &Value) -> String {
    item["audio_path"].as_str().expect("audio_path").to_owned()
}

mod recording {
    use super::*;

    #[tokio::test]
    async fn a_released_turn_becomes_a_valid_ogg_opus_file_and_row() {
        let rig = Rig::new().await;
        let (guard, admin, user) = (
            rig.guard("budi").await,
            rig.admin("boss").await,
            rig.user("sari").await,
        );
        let addr = rig.app.serve().await;
        let mut ws = join(addr, rig.group(), &guard).await;
        ws.json().await;
        request_floor(&mut ws).await;
        send_audio(&mut ws, 50).await;
        release(&mut ws).await;

        // Every role reads the same history.
        for token in [&guard, &admin, &user] {
            let items = rig.wait_for(token, 1).await;
            assert_eq!(items.len(), 1);
        }
        let item = &rig.wait_for(&guard, 1).await[0];
        assert!(item["id"].as_str().expect("id").starts_with("rec_"));
        assert_eq!(
            item["speaker"]["id"],
            json!(format!("{}@kawalo.test", rig.squad.user("budi")))
        );
        assert_eq!(item["speaker"]["name"], json!(rig.squad.user("budi")));
        assert_eq!(item["speaker"]["role"], "GUARD");
        assert_eq!(
            item["speaker"]["email"],
            json!(format!("{}@kawalo.test", rig.squad.user("budi")))
        );
        let duration = item["duration_ms"].as_i64().expect("duration");
        assert!((980..=1100).contains(&duration), "duration {duration}");
        assert!(item["started_at"].as_str().expect("date").ends_with('Z'));
        // A steady tone: every bar is the loudest frame of its slice, so all are full.
        let bars = base64::engine::general_purpose::STANDARD
            .decode(item["waveform"].as_str().expect("waveform"))
            .expect("base64");
        assert_eq!(bars.len(), 48);
        assert!(bars.iter().all(|&b| b >= 250), "{bars:?}");
        assert_eq!(
            audio_url(item),
            format!(
                "/api/v1/ptt/recordings/{}/audio",
                item["id"].as_str().expect("id")
            )
        );

        let (status, headers, bytes) = rig
            .raw(Method::GET, &audio_url(item), Some(&user), None)
            .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(headers[header::CONTENT_TYPE], "audio/ogg");
        assert_eq!(item["size_bytes"].as_u64(), Some(bytes.len() as u64));

        let decoded = decode(&bytes);
        let seconds = (decoded.granule as usize - decoded.pre_skip) as f64 / 48_000.0;
        assert!(
            (0.98..=1.02).contains(&seconds),
            "granule duration {seconds}"
        );
        assert!(decoded.packets >= 50);
        let audio = &decoded.pcm[decoded.pre_skip / 3..];
        assert!(audio.len() >= 16_000, "{} samples", audio.len());
        let level = rms(&audio[..16_000]);
        assert!((3_000.0..8_000.0).contains(&level), "tone energy {level}");

        // The file lives under UPLOAD_DIR/ptt/yyyy/mm and nothing else was left behind.
        let stored: String =
            sqlx::query_scalar("SELECT FilePath FROM tkpttrecording WHERE RecordingUID = ?")
                .bind(item["id"].as_str().expect("id"))
                .fetch_one(&rig.app.state.db)
                .await
                .expect("row");
        assert!(
            stored.starts_with("ptt/") && stored.ends_with(".opus"),
            "{stored}"
        );
        assert_eq!(
            std::fs::read(rig.dir.path().join(&stored)).expect("file"),
            bytes
        );
        let dir = rig
            .dir
            .path()
            .join(&stored)
            .parent()
            .expect("dir")
            .to_owned();
        assert_eq!(
            std::fs::read_dir(dir).expect("dir").count(),
            1,
            "no temp files"
        );
    }

    #[tokio::test]
    async fn an_admin_preempting_a_guard_makes_two_recordings() {
        let rig = Rig::new().await;
        let (guard, admin) = (rig.guard("budi").await, rig.admin("boss").await);
        let addr = rig.app.serve().await;
        let mut g = join(addr, rig.group(), &guard).await;
        g.json().await;
        let mut a = join(addr, rig.group(), &admin).await;
        a.json().await;

        request_floor(&mut g).await;
        send_audio(&mut g, 25).await;
        request_floor(&mut a).await;
        // The preempted guard keeps sending for a moment; none of it may land anywhere.
        send_audio(&mut g, 10).await;
        send_audio(&mut a, 25).await;
        release(&mut a).await;

        let items = rig.wait_for(&guard, 2).await;
        assert_eq!(items.len(), 2);
        assert_eq!(items[0]["speaker"]["role"], "ADMIN");
        assert_eq!(items[1]["speaker"]["role"], "GUARD");
        assert_eq!(items[1]["speaker"]["name"], json!(rig.squad.user("budi")));
        for item in &items {
            let ms = item["duration_ms"].as_i64().expect("duration");
            assert!((480..=600).contains(&ms), "{ms}");
        }
    }

    #[tokio::test]
    async fn a_dropped_connection_still_saves_the_turn() {
        let rig = Rig::new().await;
        let guard = rig.guard("budi").await;
        let addr = rig.app.serve().await;
        let mut ws = join(addr, rig.group(), &guard).await;
        ws.json().await;
        request_floor(&mut ws).await;
        send_audio(&mut ws, 20).await;
        ws.close().await;
        let items = rig.wait_for(&guard, 1).await;
        assert!((380..=500).contains(&items[0]["duration_ms"].as_i64().expect("ms")));
    }

    #[tokio::test]
    async fn the_time_out_timer_ends_and_saves_the_turn() {
        let rig = Rig::new().await;
        let router = Router::new()
            .merge(kawalo_api::features::ptt::ws_router_with_tot(
                Duration::from_millis(400),
            ))
            .with_state(rig.app.state.clone());
        let addr = serve(router).await;
        let guard = rig.guard("budi").await;
        let mut ws = join(addr, rig.group(), &guard).await;
        ws.json().await;
        request_floor(&mut ws).await;
        send_audio(&mut ws, 10).await;
        // No release: the 400 ms timer revokes the floor and the recording is written.
        loop {
            if ws.json().await["type"] == "FLOOR_REVOKED" {
                break;
            }
        }
        let items = rig.wait_for(&guard, 1).await;
        assert_eq!(items.len(), 1);
    }

    #[tokio::test]
    async fn a_turn_without_audio_leaves_nothing() {
        let rig = Rig::new().await;
        let guard = rig.guard("budi").await;
        let addr = rig.app.serve().await;
        let mut ws = join(addr, rig.group(), &guard).await;
        ws.json().await;
        request_floor(&mut ws).await;
        release(&mut ws).await;
        // Frames that are not audio (wrong opcode) do not count either.
        request_floor(&mut ws).await;
        ws.audio(&[0x11, 0, 0, 0, 0, 0, 0, 1, 2, 3]).await;
        release(&mut ws).await;
        tokio::time::sleep(Duration::from_millis(600)).await;
        assert!(
            rig.list(&guard, "").await.1["data"]["items"]
                .as_array()
                .expect("items")
                .is_empty()
        );
        assert!(!rig.dir.path().join("ptt").exists());
    }
}

mod listing {
    use super::*;

    #[tokio::test]
    async fn pages_newest_first_by_cursor_and_stays_in_the_group() {
        let rig = Rig::new().await;
        let guard = rig.guard("budi").await;
        let tag = Uuid::new_v4().simple().to_string();
        let base = Utc::now().trunc_subsecs(3) - chrono::Duration::hours(1);
        let at = |s: i64| base + chrono::Duration::seconds(s);
        // Two rows share a timestamp: the id breaks the tie.
        let ids: Vec<String> = ["a", "b", "c", "d", "e"]
            .iter()
            .map(|s| format!("rec_{tag}_{s}"))
            .collect();
        for (id, secs) in ids.iter().zip([1, 2, 3, 3, 4]) {
            rig.insert(rig.group(), id, at(secs), "ptt/x.opus").await;
        }
        rig.insert(
            &format!("{}-other", rig.group()),
            &format!("rec_{tag}_z"),
            at(9),
            "ptt/x.opus",
        )
        .await;
        rig.insert(rig.group(), &format!("rec_{tag}_gone"), at(8), "ptt/x.opus")
            .await;
        sqlx::query("UPDATE tkpttrecording SET DeletedAt = NOW() WHERE RecordingUID = ?")
            .bind(format!("rec_{tag}_gone"))
            .execute(&rig.app.state.db)
            .await
            .expect("soft delete");

        let mut seen = Vec::new();
        let mut query = "?limit=2".to_owned();
        let mut pages = 0;
        loop {
            let (status, json) = rig.list(&guard, &query).await;
            assert_eq!(status, StatusCode::OK, "{json}");
            assert_eq!(json["message"], "Riwayat rekaman PTT berhasil diambil");
            pages += 1;
            seen.extend(
                json["data"]["items"]
                    .as_array()
                    .expect("items")
                    .iter()
                    .map(|i| i["id"].as_str().expect("id").to_owned()),
            );
            match json["data"]["next_before"].as_str() {
                Some(cursor) => query = format!("?limit=2&before={cursor}"),
                None => break,
            }
        }
        assert_eq!(pages, 3);
        let expected: Vec<String> = ["e", "d", "c", "b", "a"]
            .iter()
            .map(|s| format!("rec_{tag}_{s}"))
            .collect();
        assert_eq!(seen, expected);

        // Exactly one full page: no cursor for a page that ends the history.
        let (_, json) = rig.list(&guard, "?limit=5").await;
        assert_eq!(json["data"]["items"].as_array().expect("items").len(), 5);
        // Rows saved without a waveform (all turns before it existed) say so with null.
        assert!(json["data"]["items"][0]["waveform"].is_null());
        assert!(json["data"]["next_before"].is_null());
    }

    #[tokio::test]
    async fn validates_the_query_and_requires_a_token() {
        let rig = Rig::new().await;
        let guard = rig.guard("budi").await;
        for query in [
            "?limit=0",
            "?limit=101",
            "?limit=abc",
            "?limit=-1",
            "?before=garbage",
            "?before=",
        ] {
            let (status, json) = rig.list(&guard, query).await;
            assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{query}: {json}");
            assert_eq!(json["error_code"], "VALIDATION_ERROR", "{query}");
        }
        for token in [None, Some("garbage")] {
            let (status, json) = rig
                .app
                .call("GET", "/api/v1/ptt/recordings", token, None)
                .await;
            assert_eq!(status, StatusCode::UNAUTHORIZED, "{json}");
        }
        assert_eq!(rig.list(&guard, "?limit=100").await.0, StatusCode::OK);
    }

    #[tokio::test]
    async fn default_page_size_is_twenty() {
        let rig = Rig::new().await;
        let guard = rig.guard("budi").await;
        let base = Utc::now().trunc_subsecs(3) - chrono::Duration::hours(2);
        for n in 0..22 {
            rig.insert(
                rig.group(),
                &format!("rec_{}_{n:02}", rig.squad.tag),
                base + chrono::Duration::seconds(n),
                "ptt/x.opus",
            )
            .await;
        }
        let (_, json) = rig.list(&guard, "").await;
        assert_eq!(json["data"]["items"].as_array().expect("items").len(), 20);
        assert!(json["data"]["next_before"].is_string());
    }
}

mod audio {
    use super::*;

    const ID_SUFFIX: &str = "audio";

    async fn setup(rig: &Rig) -> (String, Vec<u8>) {
        let bytes: Vec<u8> = (0..1000u32).map(|i| (i % 251) as u8).collect();
        let id = format!("rec_{}_{ID_SUFFIX}", rig.squad.tag);
        let path = format!("ptt/2026/11/{}.opus", rig.squad.tag);
        rig.write(&path, &bytes);
        rig.insert(rig.group(), &id, Utc::now().trunc_subsecs(3), &path)
            .await;
        (format!("/api/v1/ptt/recordings/{id}/audio"), bytes)
    }

    #[tokio::test]
    async fn serves_the_whole_file_and_byte_ranges() {
        let rig = Rig::new().await;
        let user = rig.user("sari").await;
        let (url, bytes) = setup(&rig).await;

        let (status, headers, body) = rig.raw(Method::GET, &url, Some(&user), None).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(headers[header::CONTENT_TYPE], "audio/ogg");
        assert_eq!(headers[header::ACCEPT_RANGES], "bytes");
        assert_eq!(headers[header::CACHE_CONTROL], "private");
        assert_eq!(headers[header::CONTENT_LENGTH], "1000");
        assert_eq!(body, bytes);

        for (range, status, content_range, expected) in [
            (
                "bytes=10-19",
                StatusCode::PARTIAL_CONTENT,
                "bytes 10-19/1000",
                &bytes[10..20],
            ),
            (
                "bytes=990-",
                StatusCode::PARTIAL_CONTENT,
                "bytes 990-999/1000",
                &bytes[990..],
            ),
            (
                "bytes=-5",
                StatusCode::PARTIAL_CONTENT,
                "bytes 995-999/1000",
                &bytes[995..],
            ),
            (
                "bytes=0-0",
                StatusCode::PARTIAL_CONTENT,
                "bytes 0-0/1000",
                &bytes[..1],
            ),
            (
                "bytes=500-9999",
                StatusCode::PARTIAL_CONTENT,
                "bytes 500-999/1000",
                &bytes[500..],
            ),
        ] {
            let (got, headers, body) = rig.raw(Method::GET, &url, Some(&user), Some(range)).await;
            assert_eq!(got, status, "{range}");
            assert_eq!(headers[header::CONTENT_RANGE], content_range, "{range}");
            assert_eq!(
                headers[header::CONTENT_LENGTH],
                expected.len().to_string().as_str()
            );
            assert_eq!(body, expected, "{range}");
        }

        let (status, headers, body) = rig
            .raw(Method::GET, &url, Some(&user), Some("bytes=1000-"))
            .await;
        assert_eq!(status, StatusCode::RANGE_NOT_SATISFIABLE);
        assert_eq!(headers[header::CONTENT_RANGE], "bytes */1000");
        assert!(body.is_empty());

        // A header that is not a single byte range is ignored, the whole file comes back.
        for range in ["items=0-1", "bytes=0-1,5-6", "bytes=9-3"] {
            let (status, _, body) = rig.raw(Method::GET, &url, Some(&user), Some(range)).await;
            assert_eq!((status, body.len()), (StatusCode::OK, 1000), "{range}");
        }
    }

    #[tokio::test]
    async fn needs_a_token_and_the_right_group() {
        let rig = Rig::new().await;
        let (url, _) = setup(&rig).await;
        let (status, _, _) = rig.raw(Method::GET, &url, None, None).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);

        let outsider = rig
            .app
            .token_for(
                &format!("out-{}@kawalo.test", rig.squad.tag),
                &format!("{}-other", rig.group()),
                true,
            )
            .await;
        let (status, _, body) = rig.raw(Method::GET, &url, Some(&outsider), None).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        let json: Value = serde_json::from_slice(&body).expect("json error");
        assert_eq!(json["message"], "Rekaman tidak ditemukan.");

        let guard = rig.guard("budi").await;
        let (status, _, _) = rig
            .raw(
                Method::GET,
                "/api/v1/ptt/recordings/rec_missing/audio",
                Some(&guard),
                None,
            )
            .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn never_serves_a_path_outside_the_upload_root() {
        let rig = Rig::new().await;
        let guard = rig.guard("budi").await;
        let secret = tempfile::NamedTempFile::new().expect("secret");
        std::fs::write(secret.path(), b"secret").expect("write");
        let id = format!("rec_{}_trav", rig.squad.tag);
        let escape = format!("../{}", secret.path().display());
        for (n, path) in [escape.as_str(), secret.path().to_str().expect("utf8")]
            .iter()
            .enumerate()
        {
            let id = format!("{id}{n}");
            rig.insert(rig.group(), &id, Utc::now().trunc_subsecs(3), path)
                .await;
            let (status, _, body) = rig
                .raw(
                    Method::GET,
                    &format!("/api/v1/ptt/recordings/{id}/audio"),
                    Some(&guard),
                    None,
                )
                .await;
            assert_eq!(status, StatusCode::NOT_FOUND, "{path}");
            assert!(!String::from_utf8_lossy(&body).contains("secret"));
        }
    }

    #[tokio::test]
    async fn a_row_whose_file_is_gone_is_a_404() {
        let rig = Rig::new().await;
        let guard = rig.guard("budi").await;
        let id = format!("rec_{}_nofile", rig.squad.tag);
        rig.insert(
            rig.group(),
            &id,
            Utc::now().trunc_subsecs(3),
            "ptt/2026/11/none.opus",
        )
        .await;
        let (status, _, _) = rig
            .raw(
                Method::GET,
                &format!("/api/v1/ptt/recordings/{id}/audio"),
                Some(&guard),
                None,
            )
            .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }
}

mod deleting {
    use super::*;

    const FORBIDDEN: &str = "Hanya administrator yang dapat menghapus rekaman.";

    async fn seeded(rig: &Rig) -> (String, String, PathBuf) {
        let id = format!("rec_{}_del", rig.squad.tag);
        let relative = format!("ptt/2026/11/{}.opus", rig.squad.tag);
        let file = rig.write(&relative, b"OggS-not-really");
        rig.insert(rig.group(), &id, Utc::now().trunc_subsecs(3), &relative)
            .await;
        (id.clone(), format!("/api/v1/ptt/recordings/{id}"), file)
    }

    #[tokio::test]
    async fn an_admin_soft_deletes_the_row_and_removes_the_file() {
        let rig = Rig::new().await;
        let admin = rig.admin("boss").await;
        let (id, url, file) = seeded(&rig).await;

        let (status, json) = rig.app.call("DELETE", &url, Some(&admin), None).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            json,
            json!({"success": true, "message": "Rekaman berhasil dihapus", "data": null})
        );
        assert!(rig.deleted_at(&id).await.is_some(), "tombstone stays");
        assert!(!file.exists(), "file removed");
        assert!(rig.wait_for(&admin, 0).await.is_empty());
        let (status, _, _) = rig
            .raw(Method::GET, &format!("{url}/audio"), Some(&admin), None)
            .await;
        assert_eq!(status, StatusCode::NOT_FOUND);

        // Like the incident delete, a second delete finds nothing.
        let (status, json) = rig.app.call("DELETE", &url, Some(&admin), None).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(json["error_code"], "RESOURCE_NOT_FOUND");
        assert_eq!(json["message"], "Rekaman tidak ditemukan.");
    }

    #[tokio::test]
    async fn guards_and_users_are_refused_and_nothing_changes() {
        let rig = Rig::new().await;
        let (id, url, file) = seeded(&rig).await;
        for token in [rig.guard("budi").await, rig.user("sari").await] {
            let (status, json) = rig.app.call("DELETE", &url, Some(&token), None).await;
            assert_eq!(status, StatusCode::FORBIDDEN);
            assert_eq!(json["error_code"], "AUTH_FORBIDDEN");
            assert_eq!(json["message"], FORBIDDEN);
        }
        let (status, _) = rig.app.call("DELETE", &url, None, None).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert!(rig.deleted_at(&id).await.is_none());
        assert!(file.exists());
    }

    #[tokio::test]
    async fn the_role_decides_not_the_allow_admin_flag() {
        let rig = Rig::new().await;
        let (id, url, file) = seeded(&rig).await;
        let flagged_guard = flagged_token(
            &rig.app,
            &format!("flag-{}@kawalo.test", rig.squad.tag),
            "GUARD",
            rig.group(),
            true,
        )
        .await;
        let (status, json) = rig
            .app
            .call("DELETE", &url, Some(&flagged_guard), None)
            .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{json}");
        assert_eq!(json["message"], FORBIDDEN);
        assert!(rig.deleted_at(&id).await.is_none());
        assert!(file.exists());

        let super_admin = flagged_token(
            &rig.app,
            &format!("super-{}@kawalo.test", rig.squad.tag),
            "super_admin",
            rig.group(),
            false,
        )
        .await;
        let (status, json) = rig.app.call("DELETE", &url, Some(&super_admin), None).await;
        assert_eq!(status, StatusCode::OK, "{json}");
        assert!(rig.deleted_at(&id).await.is_some());
    }

    #[tokio::test]
    async fn an_admin_of_another_group_cannot_delete_it() {
        let rig = Rig::new().await;
        let (id, url, file) = seeded(&rig).await;
        let outsider = rig
            .app
            .token_for(
                &format!("out-{}@kawalo.test", rig.squad.tag),
                &format!("{}-other", rig.group()),
                true,
            )
            .await;
        let (status, json) = rig.app.call("DELETE", &url, Some(&outsider), None).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{json}");
        assert!(rig.deleted_at(&id).await.is_none());
        assert!(file.exists());
    }

    #[tokio::test]
    async fn works_with_recording_switched_off_and_tolerates_file_trouble() {
        let rig = Rig::new().await;
        let admin = rig.admin("boss").await;
        let (id, url, file) = seeded(&rig).await;
        std::fs::remove_file(&file).expect("file already gone");
        let (status, _) = rig.app.call("DELETE", &url, Some(&admin), None).await;
        assert_eq!(status, StatusCode::OK, "a missing file is not an error");
        assert!(rig.deleted_at(&id).await.is_some());

        // A stored path that escapes the root is never touched, yet the delete succeeds.
        let secret = tempfile::NamedTempFile::new().expect("secret");
        let id = format!("rec_{}_esc", rig.squad.tag);
        rig.insert(
            rig.group(),
            &id,
            Utc::now().trunc_subsecs(3),
            secret.path().to_str().expect("utf8"),
        )
        .await;
        let (status, _) = rig
            .app
            .call(
                "DELETE",
                &format!("/api/v1/ptt/recordings/{id}"),
                Some(&admin),
                None,
            )
            .await;
        assert_eq!(status, StatusCode::OK);
        assert!(secret.path().exists(), "outside the root, left alone");
        assert!(rig.deleted_at(&id).await.is_some());
    }
}
