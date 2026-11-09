//! Incident thread events: the Redis pub/sub payload and the SSE frames derived from it.

use bytes::Bytes;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::dto::ReplyItem;
use crate::core::clean::iso_string;

/// Serde adapter writing timestamps like `Date.prototype.toISOString` so events published here
/// and by the TypeScript server are interchangeable.
pub mod iso_millis {
    use super::{DateTime, Utc, iso_string};
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(
        date: &DateTime<Utc>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&iso_string(*date))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<DateTime<Utc>, D::Error> {
        let text = String::deserialize(deserializer)?;
        DateTime::parse_from_rfc3339(&text)
            .map(|date| date.with_timezone(&Utc))
            .map_err(serde::de::Error::custom)
    }
}

/// A reply in the Redis event payload (camelCase, as the TypeScript entity).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplyEntity {
    pub id: String,
    pub incident_id: String,
    pub sender_email: String,
    pub sender_name: String,
    pub sender_role: String,
    pub message: String,
    pub photos: Vec<String>,
    #[serde(with = "iso_millis")]
    pub created_at: DateTime<Utc>,
}

/// What is published on `incident:{id}:events`; the JSON shape matches the TypeScript server.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum IncidentEvent {
    NewReply {
        reply: ReplyEntity,
    },
    #[serde(rename_all = "camelCase")]
    ThreadResolved {
        incident_id: String,
        resolved_by: String,
        resolved_by_name: String,
        #[serde(with = "iso_millis")]
        resolved_at: DateTime<Utc>,
    },
    #[serde(rename_all = "camelCase")]
    IncidentDeleted {
        incident_id: String,
        #[serde(with = "iso_millis")]
        deleted_at: DateTime<Utc>,
    },
}

/// Sent when the app must refetch the thread: on reconnect, and when a slow client lost events.
pub const SYNC_REQUIRED_FRAME: &str = "event: sync_required\ndata: {}\n\n";

impl IncidentEvent {
    /// The SSE frame for this event. Names and payloads must match
    /// `IncidentReplyMapper.parseSseMessage` in the app. Only `new_reply` carries an `id`.
    pub fn sse_frame(&self) -> Bytes {
        let frame = match self {
            Self::NewReply { reply } => format!(
                "id: {}\nevent: new_reply\ndata: {}\n\n",
                reply.id,
                json(&ReplyItem::from(reply))
            ),
            Self::ThreadResolved {
                incident_id,
                resolved_by,
                resolved_by_name,
                resolved_at,
            } => format!(
                "event: thread_resolved\ndata: {}\n\n",
                json(&serde_json::json!({
                    "incident_id": incident_id,
                    "resolved_by": resolved_by,
                    "resolved_by_name": resolved_by_name,
                    "resolved_at": iso_string(*resolved_at),
                }))
            ),
            Self::IncidentDeleted {
                incident_id,
                deleted_at,
            } => format!(
                "event: incident_deleted\ndata: {}\n\n",
                json(&serde_json::json!({
                    "incident_id": incident_id,
                    "deleted_at": iso_string(*deleted_at),
                }))
            ),
        };
        Bytes::from(frame)
    }
}

fn json(value: &impl Serialize) -> String {
    // Plain data structs and `Value` maps with string keys always serialise.
    serde_json::to_string(value).expect("event payload is serialisable")
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn at(secs: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(secs, 0)
            .single()
            .expect("valid timestamp")
    }

    fn reply() -> ReplyEntity {
        ReplyEntity {
            id: "rpl_1".into(),
            incident_id: "inc_1".into(),
            sender_email: "a@x.id".into(),
            sender_name: "Andi".into(),
            sender_role: "GUARD".into(),
            message: "m".into(),
            photos: vec![],
            created_at: at(1_767_225_600),
        }
    }

    #[test]
    fn redis_payload_matches_the_typescript_shape() {
        let new_reply = IncidentEvent::NewReply { reply: reply() };
        assert_eq!(
            serde_json::to_string(&new_reply).expect("json"),
            r#"{"type":"new_reply","reply":{"id":"rpl_1","incidentId":"inc_1","senderEmail":"a@x.id","senderName":"Andi","senderRole":"GUARD","message":"m","photos":[],"createdAt":"2026-01-01T00:00:00.000Z"}}"#
        );
        let resolved = IncidentEvent::ThreadResolved {
            incident_id: "inc_1".into(),
            resolved_by: "a@x.id".into(),
            resolved_by_name: "Andi".into(),
            resolved_at: at(0),
        };
        assert_eq!(
            serde_json::to_string(&resolved).expect("json"),
            r#"{"type":"thread_resolved","incidentId":"inc_1","resolvedBy":"a@x.id","resolvedByName":"Andi","resolvedAt":"1970-01-01T00:00:00.000Z"}"#
        );
        let parsed: IncidentEvent = serde_json::from_str(
            r#"{"type":"incident_deleted","incidentId":"inc_1","deletedAt":"1970-01-01T00:00:00.000Z"}"#,
        )
        .expect("typescript payload");
        assert_eq!(
            parsed,
            IncidentEvent::IncidentDeleted {
                incident_id: "inc_1".into(),
                deleted_at: at(0)
            }
        );
    }

    #[test]
    fn sse_frames_are_byte_exact() {
        let frame = IncidentEvent::NewReply { reply: reply() }.sse_frame();
        assert_eq!(
            std::str::from_utf8(&frame).expect("utf8"),
            "id: rpl_1\nevent: new_reply\ndata: {\"id\":\"rpl_1\",\"incident_id\":\"inc_1\",\"sender_id\":\"a@x.id\",\"sender_name\":\"Andi\",\"sender_role\":\"GUARD\",\"message\":\"m\",\"photos\":[],\"created_at\":\"2026-01-01T00:00:00.000Z\"}\n\n"
        );
        let deleted = IncidentEvent::IncidentDeleted {
            incident_id: "inc_1".into(),
            deleted_at: at(0),
        }
        .sse_frame();
        assert_eq!(
            std::str::from_utf8(&deleted).expect("utf8"),
            "event: incident_deleted\ndata: {\"incident_id\":\"inc_1\",\"deleted_at\":\"1970-01-01T00:00:00.000Z\"}\n\n"
        );
    }
}
