//! `GET /ws/ptt/{groupUid}`: handshake authentication and the per-socket read/write loops.

use std::sync::{Arc, OnceLock};
use std::time::Duration;

use axum::extract::ws::rejection::WebSocketUpgradeRejection;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Extension, Path, RawQuery, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use futures_util::{SinkExt, StreamExt};

use crate::core::error::ApiError;
use crate::core::state::AppState;

use super::recorder::Recorder;
use super::registry::{ClientTx, payload_len};
use super::session::{Hub, UserSession};

/// Largest accepted message. Opus frames are a few hundred bytes; the cap bounds what one
/// misbehaving client can make the server buffer.
const MAX_MESSAGE_BYTES: usize = 64 * 1024;

/// Close code for an unexpected failure while setting up a connection.
const CLOSE_INTERNAL_ERROR: u16 = 1011;

/// Created with the router and filled on the first connection, when the app state is available.
#[derive(Debug)]
pub struct HubCell {
    tot_duration: Duration,
    hub: OnceLock<Hub>,
}

impl HubCell {
    pub fn new(tot_duration: Duration) -> Self {
        Self {
            tot_duration,
            hub: OnceLock::new(),
        }
    }

    fn get(&self, state: &AppState) -> &Hub {
        self.hub.get_or_init(|| {
            let recorder = Recorder::new(state.db.clone(), state.config.upload_dir.clone());
            Hub::start(
                state.redis_client.clone(),
                state.redis.clone(),
                self.tot_duration,
                recorder,
            )
        })
    }
}

/// Removes a leading `Bearer ` (any case, any whitespace) like the TypeScript
/// `replace(/^Bearer\s+/i, '')`.
fn strip_bearer(value: &str) -> &str {
    match value.split_at_checked(6) {
        Some((scheme, rest))
            if scheme.eq_ignore_ascii_case("bearer") && rest.starts_with(char::is_whitespace) =>
        {
            rest.trim_start()
        }
        _ => value,
    }
}

/// Token precedence: `?token=`, then `Authorization`, then `Sec-WebSocket-Protocol` (browsers
/// cannot set headers on a WebSocket). Empty values are skipped.
fn raw_token(query: Option<&str>, headers: &HeaderMap) -> Option<String> {
    let from_query = query.and_then(|q| {
        form_urlencoded::parse(q.as_bytes())
            .find(|(key, _)| key == "token")
            .map(|(_, value)| value.into_owned())
    });
    let header_value = |name| headers.get(name).and_then(|v| v.to_str().ok());
    from_query
        .filter(|t| !t.is_empty())
        .or_else(|| {
            header_value(header::AUTHORIZATION)
                .map(strip_bearer)
                .filter(|t| !t.is_empty())
                .map(str::to_owned)
        })
        .or_else(|| {
            header_value(header::SEC_WEBSOCKET_PROTOCOL)
                .filter(|t| !t.is_empty())
                .map(str::to_owned)
        })
}

/// Handshake failures are plain text, as the TypeScript route returned them.
fn reject(status: StatusCode, message: &'static str) -> Response {
    (status, [(header::CONTENT_TYPE, "text/plain")], message).into_response()
}

pub async fn upgrade(
    State(state): State<AppState>,
    Extension(cell): Extension<Arc<HubCell>>,
    Path(group): Path<String>,
    RawQuery(query): RawQuery,
    headers: HeaderMap,
    upgrade: Result<WebSocketUpgrade, WebSocketUpgradeRejection>,
) -> Response {
    // Elysia only serves this route to upgrade requests; anything else is an unknown route.
    let Ok(upgrade) = upgrade else {
        return ApiError::route_not_found().into_response();
    };
    let Some(token) = raw_token(query.as_deref(), &headers) else {
        return reject(
            StatusCode::UNAUTHORIZED,
            "Unauthorized: Missing access token",
        );
    };
    let Some(payload) = crate::core::security::verify_access_token(&state.keys, &token) else {
        return reject(
            StatusCode::UNAUTHORIZED,
            "Unauthorized: Invalid access token signature",
        );
    };
    let Some(sub) = payload.sub.clone().filter(|sub| !sub.is_empty()) else {
        return reject(
            StatusCode::UNAUTHORIZED,
            "Unauthorized: Invalid access token signature",
        );
    };
    match state
        .sessions
        .validate_or_rehydrate(&sub, payload.session_ver)
        .await
    {
        Ok(true) => {}
        Ok(false) => {
            return reject(
                StatusCode::UNAUTHORIZED,
                "Unauthorized: Session revoked or expired",
            );
        }
        Err(err) => return ApiError::from(err).into_response(),
    }
    let other_group = payload
        .group_uid
        .as_deref()
        .is_some_and(|g| !g.is_empty() && g != group);
    if payload.role.as_deref() != Some("SUPER_ADMIN") && other_group {
        return reject(
            StatusCode::FORBIDDEN,
            "Forbidden: User does not belong to this squad group",
        );
    }

    let hub = cell.get(&state).clone();
    let user = UserSession::new(sub, payload.email, payload.name, payload.role);
    upgrade
        .max_message_size(MAX_MESSAGE_BYTES)
        .max_frame_size(MAX_MESSAGE_BYTES)
        .write_buffer_size(0)
        .on_upgrade(move |socket| serve(hub, group, user, socket))
}

async fn serve(hub: Hub, group: String, user: UserSession, socket: WebSocket) {
    let (mut sink, mut stream) = socket.split();
    let (tx, mut outbox) = ClientTx::channel();

    let mut writer = tokio::spawn(async move {
        while let Some(message) = outbox.rx.recv().await {
            let len = payload_len(&message);
            let closing = matches!(message, Message::Close(_));
            let sent = sink.send(message).await;
            outbox.written(len);
            if sent.is_err() || closing {
                break;
            }
        }
        let _ = sink.close().await;
    });

    let (session, announced) = hub.connect(&group, user, tx).await;
    if let Err(err) = announced {
        tracing::error!(error = %err, "[PTT] error during WebSocket open lifecycle");
        session.close(CLOSE_INTERNAL_ERROR, "Internal error");
    }

    loop {
        tokio::select! {
            incoming = stream.next() => match incoming {
                Some(Ok(Message::Binary(frame))) => session.on_binary(&frame),
                Some(Ok(Message::Text(text))) => {
                    if let Err(err) = session.on_text(text.as_str()).await {
                        tracing::warn!(error = %err, "[PTT] control frame failed");
                    }
                }
                Some(Ok(Message::Ping(_) | Message::Pong(_))) => {}
                Some(Ok(Message::Close(_)) | Err(_)) | None => break,
            },
            _ = &mut writer => break,
        }
    }
    session.disconnect().await;
}

#[cfg(test)]
mod tests {
    use axum::http::HeaderValue;

    use super::*;

    #[test]
    fn bearer_prefix_is_stripped_like_the_regex() {
        assert_eq!(strip_bearer("Bearer abc"), "abc");
        assert_eq!(strip_bearer("bearer   abc"), "abc");
        assert_eq!(strip_bearer("BEARER\tabc"), "abc");
        assert_eq!(strip_bearer("Bearerabc"), "Bearerabc");
        assert_eq!(strip_bearer("abc"), "abc");
        assert_eq!(strip_bearer("Bearer "), "");
        assert_eq!(strip_bearer("Béarer abc"), "Béarer abc");
    }

    #[test]
    fn token_sources_follow_the_documented_precedence() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer hdr"),
        );
        headers.insert(
            header::SEC_WEBSOCKET_PROTOCOL,
            HeaderValue::from_static("proto"),
        );
        assert_eq!(
            raw_token(Some("token=qry"), &headers).as_deref(),
            Some("qry")
        );
        assert_eq!(raw_token(Some("token="), &headers).as_deref(), Some("hdr"));
        assert_eq!(raw_token(None, &headers).as_deref(), Some("hdr"));
        headers.remove(header::AUTHORIZATION);
        assert_eq!(raw_token(Some("x=1"), &headers).as_deref(), Some("proto"));
        headers.remove(header::SEC_WEBSOCKET_PROTOCOL);
        assert_eq!(raw_token(None, &headers), None);
    }

    #[test]
    fn query_token_is_percent_decoded() {
        assert_eq!(
            raw_token(Some("a=b&token=x%2By"), &HeaderMap::new()).as_deref(),
            Some("x+y")
        );
    }
}
