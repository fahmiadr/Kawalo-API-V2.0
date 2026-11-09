//! Wire frames of the PTT protocol.
//!
//! Field names and values must stay byte-compatible with the TypeScript server (the Flutter app
//! and mixed-version clusters both parse them).

use axum::extract::ws::Utf8Bytes;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A floor holder as sent to clients and stored in Redis.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Speaker {
    #[serde(default)]
    pub user_id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub role: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FloorState {
    Idle,
    Transmitting,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DenyReason {
    ChannelBusy,
    InsufficientPriority,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RevokeReason {
    AdminOverride,
    TimeOutTimerExpired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ResetReason {
    PreemptionFlush,
}

/// Server-to-client frames.
#[derive(Debug, Serialize)]
#[serde(
    tag = "type",
    rename_all = "SCREAMING_SNAKE_CASE",
    rename_all_fields = "camelCase"
)]
pub enum Frame<'a> {
    FloorGranted {
        /// Echoed verbatim; the TypeScript server never validated its type.
        #[serde(skip_serializing_if = "Option::is_none")]
        request_id: Option<&'a Value>,
        lease_ttl_ms: u32,
    },
    FloorDenied {
        #[serde(skip_serializing_if = "Option::is_none")]
        request_id: Option<&'a Value>,
        reason: DenyReason,
        #[serde(skip_serializing_if = "Option::is_none")]
        current_speaker: Option<&'a Speaker>,
    },
    FloorStateChanged {
        state: FloorState,
        speaker: Option<&'a Speaker>,
        member_count: u64,
    },
    FloorRevoked {
        reason: RevokeReason,
        #[serde(skip_serializing_if = "Option::is_none")]
        preempted_by: Option<&'a str>,
    },
    FloorReset {
        reason: ResetReason,
    },
    MemberCountUpdated {
        member_count: u64,
    },
}

impl Frame<'_> {
    /// Serialises once; the result is cheap to clone for every listener.
    pub fn to_text(&self) -> Utf8Bytes {
        serde_json::to_string(self)
            .expect("frames contain only strings, numbers and JSON values")
            .into()
    }
}

/// Client-to-server control frames. The TypeScript server cast the parsed JSON without
/// validating it, so anything else is silently ignored here too.
#[derive(Debug, PartialEq)]
pub enum Inbound {
    FloorRequest { request_id: Option<Value> },
    FloorReleased,
    Unknown,
}

/// `None` when `text` is not JSON at all.
pub fn parse_inbound(text: &str) -> Option<Inbound> {
    let value: Value = serde_json::from_str(text).ok()?;
    Some(match value.get("type").and_then(Value::as_str) {
        Some("FLOOR_REQUEST") => Inbound::FloorRequest {
            request_id: value.get("requestId").cloned(),
        },
        Some("FLOOR_RELEASED") => Inbound::FloorReleased,
        _ => Inbound::Unknown,
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn json_of(frame: &Frame<'_>) -> Value {
        serde_json::from_str(frame.to_text().as_str()).expect("valid JSON")
    }

    fn speaker() -> Speaker {
        Speaker {
            user_id: "u1".into(),
            name: "Budi".into(),
            role: "GUARD".into(),
        }
    }

    #[test]
    fn granted_echoes_request_id_and_omits_it_when_absent() {
        let id = json!("req-1");
        assert_eq!(
            json_of(&Frame::FloorGranted {
                request_id: Some(&id),
                lease_ttl_ms: 5000
            }),
            json!({"type": "FLOOR_GRANTED", "requestId": "req-1", "leaseTtlMs": 5000})
        );
        assert_eq!(
            json_of(&Frame::FloorGranted {
                request_id: None,
                lease_ttl_ms: 5000
            }),
            json!({"type": "FLOOR_GRANTED", "leaseTtlMs": 5000})
        );
    }

    #[test]
    fn denied_carries_reason_and_current_speaker() {
        let id = json!("r");
        let who = speaker();
        assert_eq!(
            json_of(&Frame::FloorDenied {
                request_id: Some(&id),
                reason: DenyReason::InsufficientPriority,
                current_speaker: Some(&who)
            }),
            json!({
                "type": "FLOOR_DENIED", "requestId": "r", "reason": "INSUFFICIENT_PRIORITY",
                "currentSpeaker": {"userId": "u1", "name": "Budi", "role": "GUARD"}
            })
        );
        assert_eq!(
            json_of(&Frame::FloorDenied {
                request_id: Some(&id),
                reason: DenyReason::ChannelBusy,
                current_speaker: None
            }),
            json!({"type": "FLOOR_DENIED", "requestId": "r", "reason": "CHANNEL_BUSY"})
        );
    }

    #[test]
    fn state_changed_has_null_speaker_when_idle() {
        assert_eq!(
            json_of(&Frame::FloorStateChanged {
                state: FloorState::Idle,
                speaker: None,
                member_count: 3
            }),
            json!({"type": "FLOOR_STATE_CHANGED", "state": "IDLE", "speaker": null, "memberCount": 3})
        );
    }

    #[test]
    fn revoked_reset_and_member_count_names() {
        assert_eq!(
            json_of(&Frame::FloorRevoked {
                reason: RevokeReason::AdminOverride,
                preempted_by: Some("Admin")
            }),
            json!({"type": "FLOOR_REVOKED", "reason": "ADMIN_OVERRIDE", "preemptedBy": "Admin"})
        );
        assert_eq!(
            json_of(&Frame::FloorRevoked {
                reason: RevokeReason::TimeOutTimerExpired,
                preempted_by: None
            }),
            json!({"type": "FLOOR_REVOKED", "reason": "TIME_OUT_TIMER_EXPIRED"})
        );
        assert_eq!(
            json_of(&Frame::FloorReset {
                reason: ResetReason::PreemptionFlush
            }),
            json!({"type": "FLOOR_RESET", "reason": "PREEMPTION_FLUSH"})
        );
        assert_eq!(
            json_of(&Frame::MemberCountUpdated { member_count: 2 }),
            json!({"type": "MEMBER_COUNT_UPDATED", "memberCount": 2})
        );
    }

    #[test]
    fn inbound_parsing() {
        assert_eq!(
            parse_inbound(r#"{"type":"FLOOR_REQUEST","requestId":"a"}"#),
            Some(Inbound::FloorRequest {
                request_id: Some(json!("a"))
            })
        );
        assert_eq!(
            parse_inbound(r#"{"type":"FLOOR_REQUEST"}"#),
            Some(Inbound::FloorRequest { request_id: None })
        );
        assert_eq!(
            parse_inbound(r#"{"type":"FLOOR_RELEASED","reason":"x"}"#),
            Some(Inbound::FloorReleased)
        );
        assert_eq!(parse_inbound(r#"{"type":"NOPE"}"#), Some(Inbound::Unknown));
        assert_eq!(parse_inbound("42"), Some(Inbound::Unknown));
        assert_eq!(parse_inbound("null"), Some(Inbound::Unknown));
        assert_eq!(parse_inbound("not json"), None);
    }
}
