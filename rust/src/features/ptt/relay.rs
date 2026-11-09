//! Cross-instance relay over Redis pub/sub.
//!
//! Each process only knows its own sockets, so squad members connected to another instance would
//! otherwise receive neither audio nor control frames. Local fan-out stays direct; the relay only
//! forwards to OTHER instances:
//!
//! - every message carries the sender's instance id and receivers drop their own;
//! - two channels per group, `ptt:room:{group}:ctl` (JSON) and `ptt:room:{group}:audio` (the
//!   instance id as 16 ASCII bytes followed by the Opus frame);
//! - a group is subscribed only while at least one local socket is in it.
//!
//! Wire formats match the TypeScript relay so both can share a cluster.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::ws::{Message, Utf8Bytes};
use bytes::Bytes;
use futures_util::StreamExt;
use redis::aio::ConnectionManager;
use redis::{Msg, RedisError};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::{mpsc, oneshot};

use super::frames::Frame;
use super::registry::{Registry, Room};

/// Length of the instance id prefixed to each relayed audio frame.
const ORIGIN_LEN: usize = 16;

/// Publications waiting for the publisher task; beyond this the newest are dropped, which only
/// happens when Redis is unreachable.
const PUBLISH_QUEUE: usize = 8192;

/// Most publications sent in one pipeline.
const PUBLISH_BATCH: usize = 128;

const RECONNECT_MIN: Duration = Duration::from_millis(250);
const RECONNECT_MAX: Duration = Duration::from_secs(5);

fn ctl_channel(group: &str) -> String {
    format!("ptt:room:{group}:ctl")
}

fn audio_channel(group: &str) -> String {
    format!("ptt:room:{group}:audio")
}

struct Publication {
    channel: Arc<str>,
    payload: Vec<u8>,
}

enum Command {
    Join {
        group: Arc<str>,
        ack: oneshot::Sender<Result<(), RedisError>>,
    },
    Leave {
        group: Arc<str>,
    },
}

#[derive(Serialize)]
struct JsonMessage<'a> {
    o: &'a str,
    t: &'static str,
    payload: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    exclude: Option<&'a str>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PreemptedMessage<'a> {
    o: &'a str,
    t: &'static str,
    previous_user_id: &'a str,
    frame: &'a Frame<'a>,
}

/// Every field optional: unknown or malformed control messages are ignored, not errors.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ControlMessage {
    o: Option<String>,
    t: Option<String>,
    payload: Option<String>,
    exclude: Option<String>,
    previous_user_id: Option<String>,
    frame: Option<Value>,
}

/// Handle to the relay tasks. Cloning is not needed: the hub owns the only one.
#[derive(Debug)]
pub struct Relay {
    instance_id: Arc<str>,
    commands: mpsc::UnboundedSender<Command>,
    publications: mpsc::Sender<Publication>,
}

impl Relay {
    /// Starts the publisher and subscriber tasks.
    pub fn start(client: redis::Client, redis: ConnectionManager, registry: Arc<Registry>) -> Self {
        let instance_id: Arc<str> = uuid::Uuid::new_v4().simple().to_string()[..ORIGIN_LEN].into();
        let (commands, command_rx) = mpsc::unbounded_channel();
        let (publications, publication_rx) = mpsc::channel(PUBLISH_QUEUE);
        tokio::spawn(publisher(redis, publication_rx));
        tokio::spawn(subscriber(
            client,
            command_rx,
            registry,
            Arc::clone(&instance_id),
        ));
        Self {
            instance_id,
            commands,
            publications,
        }
    }

    /// Subscribes to the group's channels (idempotent) and waits until Redis confirms.
    pub async fn join(&self, group: &Arc<str>) -> Result<(), RedisError> {
        let (ack, confirmed) = oneshot::channel();
        let command = Command::Join {
            group: Arc::clone(group),
            ack,
        };
        if self.commands.send(command).is_err() {
            return Err(closed_error());
        }
        confirmed.await.unwrap_or_else(|_| Err(closed_error()))
    }

    /// Unsubscribes once no local socket remains. The subscriber re-checks the registry, so a
    /// socket that joined in the meantime keeps the subscription.
    pub fn leave(&self, group: &Arc<str>) {
        let _ = self.commands.send(Command::Leave {
            group: Arc::clone(group),
        });
    }

    /// Forwards a JSON frame to other instances; the caller has already fanned it out locally.
    pub fn publish_json(&self, group: &str, payload: &str, exclude: Option<&str>) {
        let message = JsonMessage {
            o: &self.instance_id,
            t: "json",
            payload,
            exclude,
        };
        self.publish_control(group, &message);
    }

    /// Tells other instances that `previous_user_id` lost the floor, so whichever holds their
    /// socket stops relaying their audio.
    pub fn publish_preempted(&self, group: &str, previous_user_id: &str, frame: &Frame<'_>) {
        let message = PreemptedMessage {
            o: &self.instance_id,
            t: "preempted",
            previous_user_id,
            frame,
        };
        self.publish_control(group, &message);
    }

    /// The only per-frame allocation of the audio path: the instance id has to be prepended.
    pub fn publish_binary(&self, room: &Room, frame: &[u8]) {
        let mut payload = Vec::with_capacity(ORIGIN_LEN + frame.len());
        payload.extend_from_slice(self.instance_id.as_bytes());
        payload.extend_from_slice(frame);
        self.enqueue(Publication {
            channel: Arc::clone(&room.audio_channel),
            payload,
        });
    }

    fn publish_control(&self, group: &str, message: &impl Serialize) {
        match serde_json::to_vec(message) {
            Ok(payload) => self.enqueue(Publication {
                channel: ctl_channel(group).into(),
                payload,
            }),
            Err(err) => tracing::warn!(error = %err, "[PTT] could not encode relay message"),
        }
    }

    fn enqueue(&self, publication: Publication) {
        if self.publications.try_send(publication).is_err() {
            tracing::warn!("[PTT] relay publish queue full or closed; dropping message");
        }
    }
}

fn closed_error() -> RedisError {
    RedisError::from((redis::ErrorKind::Client, "relay task stopped"))
}

/// Sends queued publications in order, batching whatever is already waiting into one pipeline.
async fn publisher(mut redis: ConnectionManager, mut queue: mpsc::Receiver<Publication>) {
    let mut batch = Vec::with_capacity(PUBLISH_BATCH);
    while queue.recv_many(&mut batch, PUBLISH_BATCH).await > 0 {
        let mut pipeline = redis::pipe();
        for publication in &batch {
            pipeline
                .cmd("PUBLISH")
                .arg(&*publication.channel)
                .arg(publication.payload.as_slice())
                .ignore();
        }
        batch.clear();
        if let Err(err) = pipeline.query_async::<()>(&mut redis).await {
            tracing::warn!(error = %err, "[PTT] relay publish failed");
        }
    }
}

/// Owns the subscriber connection: applies join/leave commands, dispatches incoming messages to
/// local sockets and reconnects (re-subscribing everything) when the connection drops.
async fn subscriber(
    client: redis::Client,
    mut commands: mpsc::UnboundedReceiver<Command>,
    registry: Arc<Registry>,
    instance_id: Arc<str>,
) {
    let mut joined: HashSet<Arc<str>> = HashSet::new();
    let mut backoff = RECONNECT_MIN;
    loop {
        let (mut sink, mut stream) = match client.get_async_pubsub().await {
            Ok(pubsub) => pubsub.split(),
            Err(err) => {
                tracing::warn!(error = %err, "[PTT] relay subscriber cannot connect");
                tokio::time::sleep(backoff).await;
                backoff = (backoff * 2).min(RECONNECT_MAX);
                continue;
            }
        };
        let channels: Vec<String> = joined
            .iter()
            .flat_map(|group| [ctl_channel(group), audio_channel(group)])
            .collect();
        if !channels.is_empty()
            && let Err(err) = sink.subscribe(&channels).await
        {
            tracing::warn!(error = %err, "[PTT] relay cannot restore subscriptions");
            tokio::time::sleep(backoff).await;
            backoff = (backoff * 2).min(RECONNECT_MAX);
            continue;
        }
        backoff = RECONNECT_MIN;

        loop {
            tokio::select! {
                command = commands.recv() => match command {
                    None => return,
                    Some(Command::Join { group, ack }) => {
                        let result = if joined.contains(&group) {
                            Ok(())
                        } else {
                            let channels = [ctl_channel(&group), audio_channel(&group)];
                            let result = sink.subscribe(&channels).await;
                            if result.is_ok() {
                                joined.insert(group);
                            }
                            result
                        };
                        let _ = ack.send(result);
                    }
                    Some(Command::Leave { group }) => {
                        if registry.client_count(&group) == 0 && joined.remove(&group) {
                            let channels = [ctl_channel(&group), audio_channel(&group)];
                            if let Err(err) = sink.unsubscribe(&channels).await {
                                tracing::warn!(error = %err, "[PTT] relay unsubscribe failed");
                            }
                        }
                    }
                },
                message = stream.next() => match message {
                    Some(message) => dispatch(&registry, &instance_id, &message),
                    None => {
                        tracing::warn!("[PTT] relay subscriber disconnected; reconnecting");
                        break;
                    }
                },
            }
        }
    }
}

fn dispatch(registry: &Registry, instance_id: &str, message: &Msg) {
    let Some((group, kind)) = message
        .get_channel_name()
        .strip_prefix("ptt:room:")
        .and_then(|rest| rest.rsplit_once(':'))
    else {
        return;
    };
    let Some(room) = registry.room(group) else {
        return;
    };
    let payload = message.get_payload_bytes();
    match kind {
        "audio" => {
            let Some((origin, frame)) = payload.split_at_checked(ORIGIN_LEN) else {
                return;
            };
            if origin == instance_id.as_bytes() {
                return;
            }
            room.broadcast_binary(&Bytes::copy_from_slice(frame), None);
        }
        "ctl" => dispatch_control(&room, instance_id, payload),
        _ => {}
    }
}

fn dispatch_control(room: &Room, instance_id: &str, payload: &[u8]) {
    let Ok(message) = serde_json::from_slice::<ControlMessage>(payload) else {
        return;
    };
    if message.o.as_deref() == Some(instance_id) {
        return;
    }
    match message.t.as_deref() {
        Some("json") => {
            if let Some(text) = message.payload {
                room.broadcast_text(&Utf8Bytes::from(text), message.exclude.as_deref());
            }
        }
        Some("preempted") => {
            let (Some(previous), Some(frame)) = (message.previous_user_id, message.frame) else {
                return;
            };
            if let Some(socket_id) = room.clear_if_speaker(&previous)
                && let Some(client) = room.client(&socket_id)
            {
                client.send(Message::Text(frame.to_string().into()));
            }
        }
        _ => {}
    }
}
