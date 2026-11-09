//! The SSE body of an incident thread.
//!
//! A reconnect with `Last-Event-ID` gets `sync_required`: the app refetches the detail, which
//! covers missed replies, resolves and deletes at once, so the server keeps no replay buffer.

use std::collections::VecDeque;
use std::convert::Infallible;
use std::time::Duration;

use bytes::Bytes;
use futures_util::Stream;
use futures_util::stream::unfold;
use tokio::sync::broadcast::error::RecvError;
use tokio::time::{Instant, MissedTickBehavior, interval_at};

use super::bus::Subscription;
use super::event::SYNC_REQUIRED_FRAME;

/// The app drops the connection after 30s of silence.
pub const HEARTBEAT: Duration = Duration::from_secs(15);

const RETRY_FRAME: &str = "retry: 3000\n\n";
const PING_FRAME: &str = ": ping\n\n";

/// Frames for one client: `retry`, then `sync_required` on a reconnect, then live events and
/// heartbeats. Dropping the stream (client gone) drops the subscription and detaches from Redis.
pub fn frames(
    subscription: Subscription,
    reconnect: bool,
    heartbeat: Duration,
) -> impl Stream<Item = Result<Bytes, Infallible>> {
    let mut preface = VecDeque::from([Bytes::from_static(RETRY_FRAME.as_bytes())]);
    if reconnect {
        preface.push_back(Bytes::from_static(SYNC_REQUIRED_FRAME.as_bytes()));
    }
    let mut ping = interval_at(Instant::now() + heartbeat, heartbeat);
    ping.set_missed_tick_behavior(MissedTickBehavior::Delay);

    unfold(
        (subscription, ping, preface),
        |(mut subscription, mut ping, mut preface)| async move {
            let frame = match preface.pop_front() {
                Some(frame) => frame,
                None => tokio::select! {
                    received = subscription.recv() => match received {
                        Ok(frame) => frame,
                        // Events were dropped for this slow client; let it refetch.
                        Err(RecvError::Lagged(_)) => Bytes::from_static(SYNC_REQUIRED_FRAME.as_bytes()),
                        Err(RecvError::Closed) => return None,
                    },
                    _ = ping.tick() => Bytes::from_static(PING_FRAME.as_bytes()),
                },
            };
            Some((Ok(frame), (subscription, ping, preface)))
        },
    )
}
