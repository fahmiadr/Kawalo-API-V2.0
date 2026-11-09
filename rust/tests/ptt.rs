//! PTT WebSocket tests against a real listener with real WebSocket clients. They need the scratch
//! services from `rust/scripts/dev-services.sh start`.

mod common;
mod ptt_support;

use std::time::Duration;

use axum::Router;
use common::TestApp;
use futures_util::StreamExt;
use ptt_support::*;
use serde_json::Value;
use serde_json::json;
use std::net::SocketAddr;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message;

mod handshake {
    use super::*;

    #[tokio::test]
    async fn refusals_are_plain_text_with_the_typescript_messages() {
        let app = TestApp::start().await;
        let addr = app.serve().await;
        let squad = Squad::new();
        let path = format!("/ws/ptt/{}", squad.group);

        let (status, content_type, body) = refused(addr, &path, &[]).await;
        assert_eq!(
            (status, content_type.as_str(), body.as_str()),
            (401, "text/plain", "Unauthorized: Missing access token")
        );

        let (status, _, body) = refused(addr, &format!("{path}?token=garbage"), &[]).await;
        assert_eq!(
            (status, body.as_str()),
            (401, "Unauthorized: Invalid access token signature")
        );

        let stale = custom_token(&app, &squad.user("stale"), "GUARD", Some(&squad.group), 0).await;
        app.state
            .sessions
            .increment_version(&squad.user("stale"))
            .await
            .expect("bump");
        let (status, _, body) = refused(addr, &format!("{path}?token={stale}"), &[]).await;
        assert_eq!(
            (status, body.as_str()),
            (401, "Unauthorized: Session revoked or expired")
        );

        let elsewhere = squad.token(&app, "guard", false).await;
        let (status, content_type, body) = refused(
            addr,
            &format!("/ws/ptt/{}-other?token={elsewhere}", squad.group),
            &[],
        )
        .await;
        assert_eq!(
            (status, content_type.as_str(), body.as_str()),
            (
                403,
                "text/plain",
                "Forbidden: User does not belong to this squad group"
            )
        );
    }

    #[tokio::test]
    async fn group_id_is_case_sensitive_and_percent_decoded() {
        let app = TestApp::start().await;
        let addr = app.serve().await;
        let squad = Squad::new();
        let token = squad.token(&app, "guard", false).await;

        let upper = format!("/ws/ptt/{}?token={token}", squad.group.to_uppercase());
        assert_eq!(refused(addr, &upper, &[]).await.0, 403);
        let encoded = format!("/ws/ptt/{}%20x?token={token}", squad.group);
        assert_eq!(refused(addr, &encoded, &[]).await.0, 403);
    }

    #[tokio::test]
    async fn token_without_group_and_super_admin_may_join_any_group() {
        let app = TestApp::start().await;
        let addr = app.serve().await;
        let squad = Squad::new();

        let no_group = custom_token(&app, &squad.user("nogroup"), "GUARD", None, 0).await;
        join(addr, &squad.group, &no_group).await.close().await;

        let hq = custom_token(
            &app,
            &squad.user("super"),
            "SUPER_ADMIN",
            Some(&squad.group),
            0,
        )
        .await;
        let mut ws = join(addr, &format!("{}-elsewhere", squad.group), &hq).await;
        assert_eq!(ws.json().await, idle(1));
    }

    #[tokio::test]
    async fn token_sources_and_their_precedence() {
        let app = TestApp::start().await;
        let addr = app.serve().await;
        let squad = Squad::new();
        let good = squad.token(&app, "guard", false).await;
        let path = format!("/ws/ptt/{}", squad.group);

        let via_header = |value: String| [("authorization", value)];
        for value in [
            format!("Bearer {good}"),
            format!("bearer   {good}"),
            good.clone(),
        ] {
            let mut ws = connect_with(addr, &path, &via_header(value))
                .await
                .expect("header token");
            assert_eq!(ws.json().await["type"], "FLOOR_STATE_CHANGED");
        }

        // The token is not echoed back as a subprotocol, exactly like the TypeScript server, so use
        // a raw handshake (tungstenite rejects a response without the requested subprotocol).
        let head = raw_handshake(addr, &path, &[("Sec-WebSocket-Protocol", &good)]).await;
        assert!(head.starts_with("HTTP/1.1 101"), "{head}");
        assert!(
            !head.to_ascii_lowercase().contains("sec-websocket-protocol"),
            "{head}"
        );

        let query_wins = format!("{path}?token={good}");
        connect_with(addr, &query_wins, &via_header("Bearer garbage".into()))
            .await
            .expect("query token beats a bad header");
        let bad_query = format!("{path}?token=garbage");
        assert_eq!(
            refused(addr, &bad_query, &via_header(format!("Bearer {good}")))
                .await
                .0,
            401
        );
        let empty_query = format!("{path}?token=");
        connect_with(addr, &empty_query, &via_header(format!("Bearer {good}")))
            .await
            .expect("empty query token falls through to the header");
    }

    async fn raw_handshake(addr: SocketAddr, path: &str, extra: &[(&str, &str)]) -> String {
        let mut socket = TcpStream::connect(addr).await.expect("connect");
        let mut request = format!(
            "GET {path} HTTP/1.1\r\nHost: {addr}\r\nConnection: Upgrade\r\nUpgrade: websocket\r\n\
             Sec-WebSocket-Version: 13\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n"
        );
        for (name, value) in extra {
            request.push_str(&format!("{name}: {value}\r\n"));
        }
        request.push_str("\r\n");
        socket.write_all(request.as_bytes()).await.expect("write");
        let mut buf = vec![0_u8; 4096];
        let read = tokio::time::timeout(WAIT, socket.read(&mut buf))
            .await
            .expect("response")
            .expect("read");
        String::from_utf8_lossy(&buf[..read]).into_owned()
    }

    #[tokio::test]
    async fn requests_that_are_not_upgrades_are_unknown_routes() {
        let app = TestApp::start().await;
        let squad = Squad::new();
        let token = squad.token(&app, "guard", false).await;
        let path = format!("/ws/ptt/{}", squad.group);

        for token in [Some(token.as_str()), None] {
            let (status, body) = app.call("GET", &path, token, None).await;
            assert_eq!(status, 404);
            assert_eq!(body["error_code"], "RESOURCE_NOT_FOUND");
        }
        let (status, _) = app.call("POST", &path, None, Some(&json!({}))).await;
        assert_eq!(status, 404);
        let (status, _) = app.call("GET", "/ws/ptt/", None, None).await;
        assert_eq!(status, 404);
    }
}

#[tokio::test]
async fn floor_control_from_grant_to_admin_preemption_and_release() {
    let app = TestApp::start().await;
    let addr = app.serve().await;
    let squad = Squad::new();
    let (tg1, tg2, ta1) = (
        squad.token(&app, "g1", false).await,
        squad.token(&app, "g2", false).await,
        squad.token(&app, "a1", true).await,
    );
    let (g1n, g2n, a1n) = (squad.user("g1"), squad.user("g2"), squad.user("a1"));

    let mut g1 = join(addr, &squad.group, &tg1).await;
    assert_eq!(g1.json().await, idle(1));
    let mut g2 = join(addr, &squad.group, &tg2).await;
    assert_eq!(g2.json().await, idle(2));
    assert_eq!(g1.json().await, count(2));
    let mut a1 = join(addr, &squad.group, &ta1).await;
    assert_eq!(a1.json().await, idle(3));
    assert_eq!(g1.json().await, count(3));
    assert_eq!(g2.json().await, count(3));

    g1.send(&json!({"type": "FLOOR_REQUEST", "requestId": "r1"}))
        .await;
    assert_eq!(
        g1.json().await,
        json!({"type": "FLOOR_GRANTED", "requestId": "r1", "leaseTtlMs": 5000})
    );
    let tx_g1 = transmitting(speaker(&g1n, "GUARD"), 3);
    for ws in [&mut g1, &mut g2, &mut a1] {
        assert_eq!(ws.json().await, tx_g1);
    }

    g2.send(&json!({"type": "FLOOR_REQUEST", "requestId": "r2"}))
        .await;
    assert_eq!(
        g2.json().await,
        json!({
            "type": "FLOOR_DENIED", "requestId": "r2", "reason": "CHANNEL_BUSY",
            "currentSpeaker": speaker(&g1n, "GUARD")
        })
    );
    g1.quiet("the speaker is not told about the denied request")
        .await;

    let frame = opus(1, 40);
    g1.audio(&frame).await;
    assert_eq!(g2.binary().await.as_ref(), frame.as_slice());
    assert_eq!(a1.binary().await.as_ref(), frame.as_slice());
    g1.quiet("the speaker does not hear itself").await;

    g2.audio(&opus(2, 40)).await;
    g1.quiet("audio from a non-speaker is dropped").await;
    a1.quiet("audio from a non-speaker is dropped").await;
    g2.send(&json!({"type": "FLOOR_RELEASED"})).await;
    g1.quiet("release by a non-speaker is ignored").await;

    a1.send(&json!({"type": "FLOOR_REQUEST", "requestId": "r3"}))
        .await;
    assert_eq!(a1.json().await["type"], "FLOOR_GRANTED");
    assert_eq!(
        g1.json().await,
        json!({"type": "FLOOR_REVOKED", "reason": "ADMIN_OVERRIDE", "preemptedBy": a1n})
    );
    let reset = json!({"type": "FLOOR_RESET", "reason": "PREEMPTION_FLUSH"});
    assert_eq!(g1.json().await, reset);
    assert_eq!(g2.json().await, reset);
    let tx_a1 = transmitting(speaker(&a1n, "ADMIN"), 3);
    for ws in [&mut a1, &mut g1, &mut g2] {
        assert_eq!(ws.json().await, tx_a1);
    }
    a1.quiet("the preempting admin gets no FLOOR_RESET").await;

    g1.audio(&opus(3, 40)).await;
    g2.quiet("in-flight audio of the preempted guard is dropped")
        .await;
    let admin_frame = opus(4, 40);
    a1.audio(&admin_frame).await;
    assert_eq!(g1.binary().await.as_ref(), admin_frame.as_slice());
    assert_eq!(g2.binary().await.as_ref(), admin_frame.as_slice());

    g1.send(&json!({"type": "FLOOR_REQUEST", "requestId": "r4"}))
        .await;
    let denied = g1.json().await;
    assert_eq!(denied["reason"], "INSUFFICIENT_PRIORITY");
    assert_eq!(denied["currentSpeaker"], speaker(&a1n, "ADMIN"));
    a1.send(&json!({"type": "FLOOR_REQUEST", "requestId": "r5"}))
        .await;
    assert_eq!(a1.json().await["reason"], "CHANNEL_BUSY");

    a1.send(&json!({"type": "FLOOR_RELEASED", "reason": "done"}))
        .await;
    for ws in [&mut a1, &mut g1, &mut g2] {
        assert_eq!(ws.json().await, idle(3));
    }

    g2.send(&json!({"type": "FLOOR_REQUEST", "requestId": "r6"}))
        .await;
    assert_eq!(g2.json().await["type"], "FLOOR_GRANTED");
    let tx_g2 = transmitting(speaker(&g2n, "GUARD"), 3);
    for ws in [&mut g2, &mut g1, &mut a1] {
        assert_eq!(ws.json().await, tx_g2);
    }

    g2.close().await;
    assert_eq!(g1.json().await, idle(2));
    assert_eq!(a1.json().await, idle(2));
    g1.close().await;
    assert_eq!(a1.json().await, count(1));
}

#[tokio::test]
async fn unusual_frames_are_tolerated_like_the_typescript_server() {
    let app = TestApp::start().await;
    let addr = app.serve().await;
    let squad = Squad::new();
    let (t1, t2) = (
        squad.token(&app, "g1", false).await,
        squad.token(&app, "g2", false).await,
    );
    let g1n = squad.user("g1");
    let mut g1 = join(addr, &squad.group, &t1).await;
    g1.json().await;
    let mut g2 = join(addr, &squad.group, &t2).await;
    g2.json().await;
    g1.json().await;

    for junk in [
        "this is not json",
        r#"{"type":"FLOOR_REQUEST""#,
        "[]",
        "null",
        r#""FLOOR_REQUEST""#,
        r#"{"type":"SOMETHING_ELSE"}"#,
        r#"{"type":"floor_request","requestId":"x"}"#,
        "{}",
    ] {
        g1.text(junk).await;
        g1.quiet(junk).await;
    }
    g1.audio(&[]).await;
    g2.quiet("empty binary frame from a non-speaker is dropped")
        .await;

    g1.send(&json!({"type": "FLOOR_REQUEST"})).await;
    assert_eq!(
        g1.json().await,
        json!({"type": "FLOOR_GRANTED", "leaseTtlMs": 5000})
    );
    g1.json().await;
    g2.json().await;

    g1.audio(&[1, 2, 3]).await;
    assert_eq!(g2.binary().await.as_ref(), [1, 2, 3]);
    let large = opus(9, 3000);
    g1.audio(&large).await;
    assert_eq!(g2.binary().await.as_ref(), large.as_slice());

    g1.text("\u{10}fallback").await;
    assert_eq!(g2.binary().await.as_ref(), b"\x10fallback");

    g1.send(&json!({"type": "FLOOR_RELEASED"})).await;
    assert_eq!(g1.json().await, idle(2));
    assert_eq!(g2.json().await, idle(2));

    g2.send(&json!({"type": "FLOOR_REQUEST", "requestId": 12345}))
        .await;
    assert_eq!(
        g2.json().await,
        json!({"type": "FLOOR_GRANTED", "requestId": 12345, "leaseTtlMs": 5000})
    );
    g2.json().await;
    assert_eq!(g1.json().await["speaker"]["name"], squad.user("g2"));
    g2.send(&json!({"type": "FLOOR_REQUEST", "requestId": ""}))
        .await;
    let denied = g2.json().await;
    assert_eq!(
        (&denied["reason"], &denied["requestId"]),
        (&json!("CHANNEL_BUSY"), &json!(""))
    );
    assert_eq!(denied["currentSpeaker"]["name"], squad.user("g2"));
    g2.send(&json!({"type": "FLOOR_RELEASED"})).await;
    assert_eq!(g2.json().await, idle(2));
    assert_eq!(g1.json().await, idle(2));
    g2.send(&json!({"type": "FLOOR_RELEASED"})).await;
    g1.quiet("release on an idle channel is ignored").await;
    let _ = g1n;
}

#[tokio::test]
async fn presence_counts_users_not_sockets_and_rooms_are_isolated() {
    let app = TestApp::start().await;
    let addr = app.serve().await;
    let squad = Squad::new();
    let (t5, t6) = (
        squad.token(&app, "g5", false).await,
        squad.token(&app, "g6", false).await,
    );

    let mut a = join(addr, &squad.group, &t5).await;
    assert_eq!(a.json().await, idle(1));
    let mut a2 = join(addr, &squad.group, &t5).await;
    assert_eq!(a2.json().await, idle(1));
    assert_eq!(a.json().await, count(1));
    let mut b = join(addr, &squad.group, &t6).await;
    assert_eq!(b.json().await, idle(2));
    assert_eq!(a.json().await, count(2));
    assert_eq!(a2.json().await, count(2));
    // Closing one socket of a user removes the user from the set, as in the TypeScript server.
    a2.close().await;
    assert_eq!(a.json().await, count(1));
    assert_eq!(b.json().await, count(1));
    b.close().await;
    assert_eq!(a.json().await, count(0));
    a.close().await;

    let other = Squad::new();
    let tok_a = squad.token(&app, "iso1", false).await;
    let tok_b = other.token(&app, "iso2", false).await;
    let mut alpha = join(addr, &squad.group, &tok_a).await;
    alpha.json().await;
    let mut beta = join(addr, &other.group, &tok_b).await;
    beta.json().await;
    alpha
        .quiet("another group joining does not reach this one")
        .await;
    beta.send(&json!({"type": "FLOOR_REQUEST", "requestId": "b1"}))
        .await;
    beta.json().await;
    beta.json().await;
    beta.audio(&opus(5, 40)).await;
    alpha
        .quiet("another group's floor and audio do not reach this one")
        .await;
}

#[tokio::test]
async fn late_joiner_sees_the_current_speaker_and_floor_frees_on_disconnect() {
    let app = TestApp::start().await;
    let addr = app.serve().await;
    let squad = Squad::new();
    let (ts, tl) = (
        squad.token(&app, "speaker", true).await,
        squad.token(&app, "late", false).await,
    );
    let mut s = join(addr, &squad.group, &ts).await;
    s.json().await;
    s.send(&json!({"type": "FLOOR_REQUEST", "requestId": "g1"}))
        .await;
    s.json().await;
    s.json().await;

    let mut late = join(addr, &squad.group, &tl).await;
    assert_eq!(
        late.json().await,
        transmitting(speaker(&squad.user("speaker"), "ADMIN"), 2)
    );
    assert_eq!(s.json().await, count(2));
    late.send(&json!({"type": "FLOOR_REQUEST", "requestId": "g2"}))
        .await;
    assert_eq!(late.json().await["reason"], "INSUFFICIENT_PRIORITY");

    s.close().await;
    assert_eq!(late.json().await, idle(1));
    let mut redis = app.state.redis.clone();
    let held: Option<String> = redis::cmd("GET")
        .arg(format!("ptt:floor:{}", squad.group))
        .query_async(&mut redis)
        .await
        .expect("get");
    assert_eq!(
        held, None,
        "the speaker's disconnect releases the Redis floor"
    );

    late.send(&json!({"type": "FLOOR_REQUEST", "requestId": "g3"}))
        .await;
    assert_eq!(late.json().await["type"], "FLOOR_GRANTED");
}

#[tokio::test]
async fn floor_lock_lives_in_redis_with_a_five_second_lease() {
    let app = TestApp::start().await;
    let addr = app.serve().await;
    let squad = Squad::new();
    let (t1, t2) = (
        squad.token(&app, "g1", false).await,
        squad.token(&app, "g2", false).await,
    );
    let mut g1 = join(addr, &squad.group, &t1).await;
    g1.json().await;
    let mut g2 = join(addr, &squad.group, &t2).await;
    g2.json().await;
    g1.json().await;

    g1.send(&json!({"type": "FLOOR_REQUEST", "requestId": "l1"}))
        .await;
    for _ in 0..2 {
        g1.json().await;
    }
    g2.json().await;

    let key = format!("ptt:floor:{}", squad.group);
    let mut redis = app.state.redis.clone();
    let ttl: i64 = redis::cmd("PTTL")
        .arg(&key)
        .query_async(&mut redis)
        .await
        .expect("pttl");
    assert!((1..=5000).contains(&ttl), "lease {ttl}ms");
    let stored: Value = serde_json::from_str(
        &redis::cmd("GET")
            .arg(&key)
            .query_async::<String>(&mut redis)
            .await
            .expect("get"),
    )
    .expect("json");
    assert_eq!(
        stored["userId"],
        format!("{}@kawalo.test", squad.user("g1"))
    );
    assert_eq!(stored["role"], "GUARD");
    assert!(stored["socketId"].as_str().is_some_and(|s| !s.is_empty()));

    // Without audio the lease lapses; the key vanishing is what the TypeScript test waits 5.6s for.
    let _: () = redis::cmd("PEXPIRE")
        .arg(&key)
        .arg(50)
        .query_async(&mut redis)
        .await
        .expect("shorten");
    tokio::time::sleep(Duration::from_millis(150)).await;
    g2.send(&json!({"type": "FLOOR_REQUEST", "requestId": "l3"}))
        .await;
    assert_eq!(g2.json().await["type"], "FLOOR_GRANTED");
    g2.json().await;
    g1.json().await;
    g1.audio(&opus(7, 40)).await;
    g2.quiet("audio of the stale speaker is dropped").await;
}

#[tokio::test]
async fn audio_renews_the_lease_at_most_once_a_second() {
    let app = TestApp::start().await;
    let addr = app.serve().await;
    let squad = Squad::new();
    let (t1, t2) = (
        squad.token(&app, "g1", false).await,
        squad.token(&app, "g2", false).await,
    );
    let mut g1 = join(addr, &squad.group, &t1).await;
    g1.json().await;
    let mut g2 = join(addr, &squad.group, &t2).await;
    g2.json().await;
    g1.json().await;
    g1.send(&json!({"type": "FLOOR_REQUEST", "requestId": "t1"}))
        .await;
    for _ in 0..2 {
        g1.json().await;
    }
    g2.json().await;

    let key = format!("ptt:floor:{}", squad.group);
    let mut redis = app.state.redis.clone();
    let _: () = redis::cmd("PEXPIRE")
        .arg(&key)
        .arg(1500)
        .query_async(&mut redis)
        .await
        .expect("shorten");
    g1.audio(&opus(1, 40)).await;
    g2.binary().await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    let ttl: i64 = redis::cmd("PTTL")
        .arg(&key)
        .query_async(&mut redis)
        .await
        .expect("pttl");
    assert!(
        ttl > 3000,
        "first frame renews the lease to 5s, got {ttl}ms"
    );

    let _: () = redis::cmd("PEXPIRE")
        .arg(&key)
        .arg(1500)
        .query_async(&mut redis)
        .await
        .expect("shorten");
    for n in 2..20 {
        g1.audio(&opus(n, 40)).await;
        g2.binary().await;
    }
    let ttl: i64 = redis::cmd("PTTL")
        .arg(&key)
        .query_async(&mut redis)
        .await
        .expect("pttl");
    assert!(
        ttl <= 1500,
        "frames within the same second must not renew again, got {ttl}ms"
    );
}

#[tokio::test]
async fn time_out_timer_revokes_a_stuck_speaker() {
    let app = TestApp::start().await;
    let router = Router::new()
        .merge(kawalo_api::features::ptt::ws_router_with_tot(
            Duration::from_millis(400),
        ))
        .with_state(app.state.clone());
    let addr = serve(router).await;
    let squad = Squad::new();
    let (t1, t2) = (
        squad.token(&app, "g1", false).await,
        squad.token(&app, "g2", false).await,
    );
    let mut g1 = join(addr, &squad.group, &t1).await;
    g1.json().await;
    let mut g2 = join(addr, &squad.group, &t2).await;
    g2.json().await;
    g1.json().await;

    g1.send(&json!({"type": "FLOOR_REQUEST", "requestId": "t1"}))
        .await;
    for _ in 0..2 {
        g1.json().await;
    }
    g2.json().await;

    assert_eq!(
        g1.json().await,
        json!({"type": "FLOOR_REVOKED", "reason": "TIME_OUT_TIMER_EXPIRED"})
    );
    assert_eq!(g1.json().await, idle(2));
    assert_eq!(g2.json().await, idle(2));
    g1.audio(&opus(1, 40)).await;
    g2.quiet("audio after the time-out is dropped").await;

    g2.send(&json!({"type": "FLOOR_REQUEST", "requestId": "t2"}))
        .await;
    assert_eq!(g2.json().await["type"], "FLOOR_GRANTED");
}

#[tokio::test]
async fn releasing_in_time_cancels_the_time_out_timer() {
    let app = TestApp::start().await;
    let router = Router::new()
        .merge(kawalo_api::features::ptt::ws_router_with_tot(
            Duration::from_millis(400),
        ))
        .with_state(app.state.clone());
    let addr = serve(router).await;
    let squad = Squad::new();
    let token = squad.token(&app, "g1", false).await;
    let mut g1 = join(addr, &squad.group, &token).await;
    g1.json().await;
    g1.send(&json!({"type": "FLOOR_REQUEST", "requestId": "t1"}))
        .await;
    g1.json().await;
    g1.json().await;
    g1.send(&json!({"type": "FLOOR_RELEASED"})).await;
    assert_eq!(g1.json().await, idle(1));
    tokio::time::sleep(Duration::from_millis(700)).await;
    g1.quiet("no TOT revocation after a release").await;
}

#[tokio::test]
async fn a_slow_listener_loses_audio_instead_of_delaying_everyone() {
    let app = TestApp::start().await;
    let addr = app.serve().await;
    let squad = Squad::new();
    let (ts, tl, tf) = (
        squad.token(&app, "speaker", false).await,
        squad.token(&app, "slow", false).await,
        squad.token(&app, "fast", false).await,
    );
    let mut speaker_ws = join(addr, &squad.group, &ts).await;
    speaker_ws.json().await;
    let mut slow = join(addr, &squad.group, &tl).await;
    slow.json().await;
    let mut fast = join(addr, &squad.group, &tf).await;
    fast.json().await;
    speaker_ws.json().await;
    speaker_ws.json().await;
    slow.json().await;

    speaker_ws
        .send(&json!({"type": "FLOOR_REQUEST", "requestId": "s"}))
        .await;
    for _ in 0..2 {
        speaker_ws.json().await;
    }
    slow.json().await;
    fast.json().await;

    // `slow` does not read: once its kernel buffers fill, its queue passes the ceiling and frames
    // are dropped for it alone while `fast` keeps up.
    let frame = opus(1, 1000);
    let sent: usize = 6000;
    let reader = tokio::spawn(async move {
        let mut received = 0_usize;
        while let Ok(Some(Ok(Message::Binary(_)))) =
            tokio::time::timeout(Duration::from_millis(500), fast.0.next()).await
        {
            received += 1;
        }
        received
    });
    for n in 0..sent {
        speaker_ws.audio(&frame).await;
        if n % 5 == 0 {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    }
    let fast_received = reader.await.expect("reader");

    let mut slow_received = 0_usize;
    while let Ok(Some(Ok(Message::Binary(_)))) =
        tokio::time::timeout(Duration::from_millis(300), slow.0.next()).await
    {
        slow_received += 1;
    }
    assert!(
        slow_received < sent,
        "the slow listener must lose frames, got {slow_received} of {sent}"
    );
    assert!(
        fast_received > slow_received,
        "the fast listener is not held back: fast {fast_received}, slow {slow_received}"
    );
}

mod cluster {
    use super::*;

    async fn numsub(app: &TestApp, channel: &str) -> i64 {
        let mut redis = app.state.redis.clone();
        let (_, count): (String, i64) = redis::cmd("PUBSUB")
            .arg("NUMSUB")
            .arg(channel)
            .query_async(&mut redis)
            .await
            .expect("numsub");
        count
    }

    async fn eventually_numsub(app: &TestApp, channel: &str, want: i64) {
        for _ in 0..40 {
            if numsub(app, channel).await == want {
                return;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!(
            "{channel} never reached {want} subscribers, has {}",
            numsub(app, channel).await
        );
    }

    #[tokio::test]
    async fn frames_and_audio_cross_instances_exactly_once() {
        let app = TestApp::start().await;
        let a = app.serve().await;
        let b = second_instance(&app).await;
        let squad = Squad::new();
        let (ta, tb, tc) = (
            squad.token(&app, "ga", false).await,
            squad.token(&app, "gb", false).await,
            squad.token(&app, "gc", false).await,
        );

        let mut ga = join(a, &squad.group, &ta).await;
        assert_eq!(ga.json().await, idle(1));
        let mut gb = join(b, &squad.group, &tb).await;
        assert_eq!(gb.json().await, idle(2));
        assert_eq!(ga.json().await, count(2));
        let mut gc = join(b, &squad.group, &tc).await;
        assert_eq!(gc.json().await, idle(3));
        assert_eq!(ga.json().await, count(3));
        assert_eq!(gb.json().await, count(3));

        ga.send(&json!({"type": "FLOOR_REQUEST", "requestId": "x"}))
            .await;
        assert_eq!(ga.json().await["type"], "FLOOR_GRANTED");
        let tx = transmitting(speaker(&squad.user("ga"), "GUARD"), 3);
        for ws in [&mut ga, &mut gb, &mut gc] {
            assert_eq!(ws.json().await, tx);
        }

        let frame = opus(1, 60);
        ga.audio(&frame).await;
        assert_eq!(gb.binary().await.as_ref(), frame.as_slice());
        assert_eq!(gc.binary().await.as_ref(), frame.as_slice());
        ga.quiet("no echo of own audio through the relay").await;
        gb.quiet("exactly one copy on the remote instance").await;
        gc.quiet("exactly one copy on the remote instance").await;

        // A listener on the other instance cannot speak while the floor is held.
        gb.audio(&opus(2, 60)).await;
        ga.quiet("audio from a non-speaker on the other instance is dropped")
            .await;

        ga.send(&json!({"type": "FLOOR_RELEASED"})).await;
        for ws in [&mut ga, &mut gb, &mut gc] {
            assert_eq!(ws.json().await, idle(3));
        }
        for ws in [&mut ga, &mut gb, &mut gc] {
            ws.quiet("each frame arrives exactly once").await;
        }
    }

    #[tokio::test]
    async fn admin_on_another_instance_preempts_and_the_old_speaker_stops_relaying() {
        let app = TestApp::start().await;
        let a = app.serve().await;
        let b = second_instance(&app).await;
        let squad = Squad::new();
        let (tg, tl, ta) = (
            squad.token(&app, "guard", false).await,
            squad.token(&app, "listener", false).await,
            squad.token(&app, "admin", true).await,
        );
        let mut guard = join(a, &squad.group, &tg).await;
        guard.json().await;
        let mut listener = join(b, &squad.group, &tl).await;
        listener.json().await;
        guard.json().await;
        let mut admin = join(b, &squad.group, &ta).await;
        admin.json().await;
        guard.json().await;
        listener.json().await;

        guard
            .send(&json!({"type": "FLOOR_REQUEST", "requestId": "g"}))
            .await;
        guard.json().await;
        guard.json().await;
        listener.json().await;
        admin.json().await;
        guard.audio(&opus(1, 40)).await;
        listener.binary().await;
        admin.binary().await;

        admin
            .send(&json!({"type": "FLOOR_REQUEST", "requestId": "a"}))
            .await;
        assert_eq!(admin.json().await["type"], "FLOOR_GRANTED");
        assert_eq!(
            guard.json().await,
            json!({
                "type": "FLOOR_REVOKED", "reason": "ADMIN_OVERRIDE",
                "preemptedBy": squad.user("admin")
            })
        );
        let reset = json!({"type": "FLOOR_RESET", "reason": "PREEMPTION_FLUSH"});
        assert_eq!(guard.json().await, reset);
        assert_eq!(listener.json().await, reset);
        let tx = transmitting(speaker(&squad.user("admin"), "ADMIN"), 3);
        for ws in [&mut admin, &mut guard, &mut listener] {
            assert_eq!(ws.json().await, tx);
        }

        guard.audio(&opus(2, 40)).await;
        listener
            .quiet("the preempted guard's later audio is dropped")
            .await;
        admin
            .quiet("the preempted guard's later audio is dropped")
            .await;
        let frame = opus(3, 40);
        admin.audio(&frame).await;
        assert_eq!(guard.binary().await.as_ref(), frame.as_slice());
        assert_eq!(listener.binary().await.as_ref(), frame.as_slice());
    }

    #[tokio::test]
    async fn an_instance_subscribes_only_while_it_has_local_sockets() {
        let app = TestApp::start().await;
        let a = app.serve().await;
        let b = second_instance(&app).await;
        let squad = Squad::new();
        let ctl = format!("ptt:room:{}:ctl", squad.group);
        let audio = format!("ptt:room:{}:audio", squad.group);
        let (t1, t2, t3) = (
            squad.token(&app, "u1", false).await,
            squad.token(&app, "u2", false).await,
            squad.token(&app, "u3", false).await,
        );

        assert_eq!(numsub(&app, &ctl).await, 0);
        let mut first = join(a, &squad.group, &t1).await;
        first.json().await;
        let mut second = join(a, &squad.group, &t2).await;
        second.json().await;
        eventually_numsub(&app, &ctl, 1).await;
        eventually_numsub(&app, &audio, 1).await;

        let mut remote = join(b, &squad.group, &t3).await;
        remote.json().await;
        eventually_numsub(&app, &ctl, 2).await;

        first.close().await;
        tokio::time::sleep(Duration::from_millis(150)).await;
        assert_eq!(
            numsub(&app, &ctl).await,
            2,
            "one local socket remains on instance A"
        );
        second.close().await;
        eventually_numsub(&app, &ctl, 1).await;
        remote.close().await;
        eventually_numsub(&app, &ctl, 0).await;
        eventually_numsub(&app, &audio, 0).await;

        let mut again = join(a, &squad.group, &t1).await;
        again.json().await;
        eventually_numsub(&app, &ctl, 1).await;
    }
}
