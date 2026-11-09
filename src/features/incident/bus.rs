//! Redis pub/sub for incident events, so an SSE client connected to another server instance
//! still receives replies.
//!
//! One background task owns a single subscriber connection shared by every SSE stream of this
//! router. A channel is subscribed when its first stream attaches and unsubscribed when the last
//! one drops its [`Subscription`]. If the connection is lost the task reconnects and resubscribes.

use std::collections::HashMap;
use std::future::pending;
use std::sync::Mutex;
use std::time::Duration;

use bytes::Bytes;
use futures_util::StreamExt;
use redis::aio::{PubSubSink, PubSubStream};
use tokio::sync::{broadcast, mpsc, oneshot};

use super::event::IncidentEvent;
use crate::core::state::AppState;

/// Frames a slow client may fall behind by before it is told to resync.
const CHANNEL_CAPACITY: usize = 64;
const RECONNECT_MIN: Duration = Duration::from_millis(250);
const RECONNECT_MAX: Duration = Duration::from_secs(5);

pub fn channel(incident_id: &str) -> String {
    format!("incident:{incident_id}:events")
}

/// Publishes an event. The data is already saved when this runs, so a failure only logs.
pub async fn publish(state: &AppState, incident_id: &str, event: &IncidentEvent) {
    let payload = match serde_json::to_string(event) {
        Ok(payload) => payload,
        Err(err) => {
            tracing::warn!(%err, incident_id, "[Incident] event could not be serialised");
            return;
        }
    };
    let mut redis = state.redis.clone();
    let result: redis::RedisResult<i64> = redis::cmd("PUBLISH")
        .arg(channel(incident_id))
        .arg(payload)
        .query_async(&mut redis)
        .await;
    if let Err(err) = result {
        tracing::warn!(%err, incident_id, "[Incident] failed to publish event");
    }
}

enum Command {
    Subscribe {
        channel: String,
        reply: oneshot::Sender<redis::RedisResult<broadcast::Receiver<Bytes>>>,
    },
    Release(String),
}

/// Live feed of ready-to-send SSE frames for one incident. Dropping it detaches from the channel.
#[derive(Debug)]
pub struct Subscription {
    // Declared first so the receiver is gone before `_release` asks the task to count receivers.
    frames: broadcast::Receiver<Bytes>,
    _release: Release,
}

impl Subscription {
    /// # Errors
    /// `Lagged` when the client fell behind and frames were dropped; `Closed` when the bus stopped.
    pub async fn recv(&mut self) -> Result<Bytes, broadcast::error::RecvError> {
        self.frames.recv().await
    }
}

#[derive(Debug)]
struct Release {
    channel: String,
    commands: mpsc::UnboundedSender<Command>,
}

impl Drop for Release {
    fn drop(&mut self) {
        // The task is gone only at shutdown, when there is nothing left to release.
        let _ = self
            .commands
            .send(Command::Release(std::mem::take(&mut self.channel)));
    }
}

/// Handle to the shared subscriber; the connection is opened on first use.
#[derive(Debug, Default)]
pub struct IncidentBus {
    commands: Mutex<Option<mpsc::UnboundedSender<Command>>>,
}

impl IncidentBus {
    /// Attaches to the incident's channel.
    ///
    /// # Errors
    /// Redis could not be reached to subscribe.
    pub async fn subscribe(
        &self,
        state: &AppState,
        incident_id: &str,
    ) -> redis::RedisResult<Subscription> {
        let commands = self.sender(&state.redis_client);
        let channel = channel(incident_id);
        let (reply, answer) = oneshot::channel();
        let closed = || redis::RedisError::from((redis::ErrorKind::Io, "incident bus stopped"));
        commands
            .send(Command::Subscribe {
                channel: channel.clone(),
                reply,
            })
            .map_err(|_| closed())?;
        let frames = answer.await.map_err(|_| closed())??;
        Ok(Subscription {
            frames,
            _release: Release { channel, commands },
        })
    }

    fn sender(&self, client: &redis::Client) -> mpsc::UnboundedSender<Command> {
        let mut slot = self.commands.lock().expect("bus lock poisoned");
        match slot.as_ref().filter(|sender| !sender.is_closed()) {
            Some(sender) => sender.clone(),
            None => {
                let (sender, receiver) = mpsc::unbounded_channel();
                tokio::spawn(run(client.clone(), receiver));
                *slot = Some(sender.clone());
                sender
            }
        }
    }
}

struct Link {
    sink: PubSubSink,
    messages: PubSubStream,
}

async fn connect(client: &redis::Client, channels: &[String]) -> redis::RedisResult<Link> {
    let (mut sink, messages) = client.get_async_pubsub().await?.split();
    for channel in channels {
        sink.subscribe(channel).await?;
    }
    Ok(Link { sink, messages })
}

async fn next_message(link: &mut Option<Link>) -> Option<redis::Msg> {
    match link {
        Some(link) => link.messages.next().await,
        None => pending().await,
    }
}

async fn when_due(deadline: Option<tokio::time::Instant>) {
    match deadline {
        Some(deadline) => tokio::time::sleep_until(deadline).await,
        None => pending().await,
    }
}

/// Owns the subscriber connection and fans messages out per channel.
async fn run(client: redis::Client, mut commands: mpsc::UnboundedReceiver<Command>) {
    let mut topics: HashMap<String, broadcast::Sender<Bytes>> = HashMap::new();
    let mut link: Option<Link> = None;
    let mut retry_at: Option<tokio::time::Instant> = None;
    let mut backoff = RECONNECT_MIN;

    loop {
        tokio::select! {
            command = commands.recv() => {
                let Some(command) = command else { return };
                match command {
                    Command::Subscribe { channel, reply } => {
                        let result = attach(&client, &mut link, &mut topics, channel).await;
                        // The requester may have gone away; the topic then drains via `Release`.
                        let _ = reply.send(result);
                        if link.is_some() {
                            retry_at = None;
                            backoff = RECONNECT_MIN;
                        }
                    }
                    Command::Release(channel) => release(&mut link, &mut topics, &channel).await,
                }
            }
            message = next_message(&mut link) => match message {
                Some(message) => dispatch(&topics, &message),
                None => {
                    tracing::warn!("[Incident] Redis subscriber connection lost; reconnecting");
                    link = None;
                    retry_at = Some(tokio::time::Instant::now() + backoff);
                }
            },
            () = when_due(retry_at) => {
                let channels: Vec<String> = topics.keys().cloned().collect();
                if channels.is_empty() {
                    retry_at = None;
                    continue;
                }
                match connect(&client, &channels).await {
                    Ok(restored) => {
                        link = Some(restored);
                        retry_at = None;
                        backoff = RECONNECT_MIN;
                    }
                    Err(err) => {
                        tracing::warn!(%err, "[Incident] Redis subscriber reconnect failed");
                        backoff = (backoff * 2).min(RECONNECT_MAX);
                        retry_at = Some(tokio::time::Instant::now() + backoff);
                    }
                }
            }
        }
    }
}

async fn attach(
    client: &redis::Client,
    link: &mut Option<Link>,
    topics: &mut HashMap<String, broadcast::Sender<Bytes>>,
    channel: String,
) -> redis::RedisResult<broadcast::Receiver<Bytes>> {
    if let Some(topic) = topics.get(&channel) {
        return Ok(topic.subscribe());
    }
    match link {
        Some(live) => live.sink.subscribe(&channel).await?,
        None => {
            let mut channels: Vec<String> = topics.keys().cloned().collect();
            channels.push(channel.clone());
            *link = Some(connect(client, &channels).await?);
        }
    }
    let (topic, frames) = broadcast::channel(CHANNEL_CAPACITY);
    topics.insert(channel, topic);
    Ok(frames)
}

async fn release(
    link: &mut Option<Link>,
    topics: &mut HashMap<String, broadcast::Sender<Bytes>>,
    channel: &str,
) {
    if topics
        .get(channel)
        .is_none_or(|topic| topic.receiver_count() > 0)
    {
        return;
    }
    topics.remove(channel);
    if let Some(live) = link
        && let Err(err) = live.sink.unsubscribe(channel).await
    {
        tracing::debug!(%err, channel, "[Incident] unsubscribe failed");
    }
}

fn dispatch(topics: &HashMap<String, broadcast::Sender<Bytes>>, message: &redis::Msg) {
    let Some(topic) = topics.get(message.get_channel_name()) else {
        return;
    };
    let event = message
        .get_payload::<String>()
        .map_err(|err| err.to_string())
        .and_then(|payload| {
            serde_json::from_str::<IncidentEvent>(&payload).map_err(|err| err.to_string())
        });
    match event {
        // No receivers just means every client left while the message was in flight.
        Ok(event) => drop(topic.send(event.sse_frame())),
        Err(err) => tracing::warn!(%err, "[Incident] invalid pub/sub event ignored"),
    }
}
