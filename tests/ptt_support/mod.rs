//! Helpers for the PTT WebSocket tests: a small client and token/squad builders.

#![allow(dead_code, reason = "each test binary uses a different subset")]

use std::net::SocketAddr;
use std::time::Duration;

use crate::common::TestApp;
use axum::Router;
use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use kawalo_api::core::security::{JwtUserPayload, sign_access_token};
use serde_json::{Value, json};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderName;
use tokio_tungstenite::tungstenite::{Error as WsError, Message};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};
use uuid::Uuid;

pub const WAIT: Duration = Duration::from_secs(3);
pub const QUIET: Duration = Duration::from_millis(250);

pub type Stream = WebSocketStream<MaybeTlsStream<TcpStream>>;

pub struct Ws(pub Stream);

impl Ws {
    pub async fn next_message(&mut self) -> Message {
        loop {
            let message = tokio::time::timeout(WAIT, self.0.next())
                .await
                .expect("a frame within 3s")
                .expect("socket still open")
                .expect("no protocol error");
            if !matches!(message, Message::Ping(_) | Message::Pong(_)) {
                return message;
            }
        }
    }

    pub async fn json(&mut self) -> Value {
        match self.next_message().await {
            Message::Text(text) => serde_json::from_str(text.as_str()).expect("JSON frame"),
            other => panic!("expected a text frame, got {other:?}"),
        }
    }

    pub async fn binary(&mut self) -> Bytes {
        match self.next_message().await {
            Message::Binary(data) => data,
            other => panic!("expected a binary frame, got {other:?}"),
        }
    }

    pub async fn closed_with(&mut self) -> Option<(u16, String)> {
        match self.next_message().await {
            Message::Close(frame) => frame.map(|f| (f.code.into(), f.reason.to_string())),
            other => panic!("expected a close frame, got {other:?}"),
        }
    }

    pub async fn quiet(&mut self, label: &str) {
        if let Ok(Some(message)) = tokio::time::timeout(QUIET, self.0.next()).await {
            panic!("{label}: unexpected {message:?}");
        }
    }

    pub async fn send(&mut self, frame: &Value) {
        self.text(&frame.to_string()).await;
    }

    pub async fn text(&mut self, text: &str) {
        self.0.send(Message::text(text)).await.expect("send text");
    }

    pub async fn audio(&mut self, frame: &[u8]) {
        self.0
            .send(Message::binary(frame.to_vec()))
            .await
            .expect("send binary");
    }

    pub async fn close(mut self) {
        let _ = self.0.close(None).await;
    }
}

pub async fn connect_with(
    addr: SocketAddr,
    path: &str,
    headers: &[(&str, String)],
) -> Result<Ws, WsError> {
    let mut request = format!("ws://{addr}{path}")
        .into_client_request()
        .expect("request");
    for (name, value) in headers {
        request.headers_mut().insert(
            HeaderName::from_bytes(name.as_bytes()).expect("header name"),
            value.parse().expect("header value"),
        );
    }
    connect_async(request).await.map(|(stream, _)| Ws(stream))
}

pub async fn join(addr: SocketAddr, group: &str, token: &str) -> Ws {
    connect_with(addr, &format!("/ws/ptt/{group}?token={token}"), &[])
        .await
        .expect("handshake accepted")
}

/// Status and body of a refused handshake.
pub async fn refused(
    addr: SocketAddr,
    path: &str,
    headers: &[(&str, String)],
) -> (u16, String, String) {
    match connect_with(addr, path, headers).await {
        Err(WsError::Http(response)) => (
            response.status().as_u16(),
            response
                .headers()
                .get("content-type")
                .map(|v| v.to_str().expect("ascii").to_owned())
                .unwrap_or_default(),
            String::from_utf8(response.body().clone().unwrap_or_default()).expect("utf-8 body"),
        ),
        Err(other) => panic!("expected an HTTP refusal, got {other:?}"),
        Ok(_) => panic!("handshake unexpectedly accepted"),
    }
}

pub async fn serve(router: Router) -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("addr");
    let app = router.into_make_service_with_connect_info::<SocketAddr>();
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    addr
}

/// A second app instance over the same state: a new socket registry and relay on the same Redis.
pub async fn second_instance(app: &TestApp) -> SocketAddr {
    serve(kawalo_api::build_app(app.state.clone())).await
}

/// Unique group and user names per test so tests can run in parallel on shared services.
pub struct Squad {
    pub group: String,
    pub tag: String,
}

impl Squad {
    pub fn new() -> Self {
        let tag = Uuid::new_v4().simple().to_string();
        Self {
            group: format!("grp-{tag}"),
            tag,
        }
    }

    pub fn user(&self, who: &str) -> String {
        format!("{who}-{}", self.tag)
    }

    pub async fn token(&self, app: &TestApp, who: &str, admin: bool) -> String {
        app.token_for(
            &format!("{}@kawalo.test", self.user(who)),
            &self.group,
            admin,
        )
        .await
    }
}

pub async fn custom_token(
    app: &TestApp,
    sub: &str,
    role: &str,
    group: Option<&str>,
    version_offset: i64,
) -> String {
    let session_ver = app
        .state
        .sessions
        .get_or_init_version(sub)
        .await
        .expect("session version")
        + version_offset;
    let payload = JwtUserPayload {
        sub: Some(sub.to_owned()),
        email: Some(sub.to_owned()),
        name: Some(sub.to_owned()),
        role: Some(role.to_owned()),
        group_uid: group.map(str::to_owned),
        session_ver,
        ..JwtUserPayload::default()
    };
    sign_access_token(&app.state.keys, app.state.config.jwt_expires_in, &payload)
        .expect("sign")
        .token
}

pub fn speaker(user: &str, role: &str) -> Value {
    json!({"userId": format!("{user}@kawalo.test"), "name": user, "role": role})
}

pub fn idle(member_count: u64) -> Value {
    json!({"type": "FLOOR_STATE_CHANGED", "state": "IDLE", "speaker": null, "memberCount": member_count})
}

pub fn transmitting(speaker: Value, member_count: u64) -> Value {
    json!({"type": "FLOOR_STATE_CHANGED", "state": "TRANSMITTING", "speaker": speaker, "memberCount": member_count})
}

pub fn count(member_count: u64) -> Value {
    json!({"type": "MEMBER_COUNT_UPDATED", "memberCount": member_count})
}

pub fn opus(n: u32, len: usize) -> Vec<u8> {
    let mut frame = vec![0x10];
    frame.extend_from_slice(&n.to_be_bytes());
    frame.extend_from_slice(&u16::try_from(len).expect("len").to_be_bytes());
    frame.extend((0..len).map(|i| u8::try_from((n as usize * 31 + i) % 256).expect("byte")));
    frame
}
