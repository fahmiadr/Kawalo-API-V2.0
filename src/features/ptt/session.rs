//! Floor control and fan-out for one connected socket, ported from `ptt.controller.ts`.

use std::sync::Arc;
use std::time::Duration;

use axum::extract::ws::Message;
use bytes::Bytes;
use serde_json::Value;

use super::frames::{
    DenyReason, FloorState, Frame, Inbound, ResetReason, RevokeReason, Speaker, parse_inbound,
};
use super::registry::{ActiveSpeaker, Client, ClientTx, Registry, Room};
use super::relay::Relay;
use super::store::{FloorLock, FloorStore, StoreError};

/// Time-out timer: a speaker is cut off after this long, which frees a stuck push-to-talk button.
pub const TOT_DURATION: Duration = Duration::from_secs(30);

/// Floor lease in Redis. Long enough to ride out network jitter and short pauses in speech; audio
/// frames renew it, so a crashed instance frees the floor within this time.
const LEASE_SECONDS: u32 = 5;

const LEASE_TTL_MS: u32 = LEASE_SECONDS * 1000;

/// Process-wide PTT state shared by every connection.
#[derive(Debug, Clone)]
pub struct Hub(Arc<HubInner>);

#[derive(Debug)]
struct HubInner {
    registry: Arc<Registry>,
    store: FloorStore,
    relay: Relay,
    tot_duration: Duration,
}

impl Hub {
    pub fn start(
        client: redis::Client,
        redis: redis::aio::ConnectionManager,
        tot_duration: Duration,
    ) -> Self {
        let registry = Arc::new(Registry::default());
        let relay = Relay::start(client, redis.clone(), Arc::clone(&registry));
        Self(Arc::new(HubInner {
            registry,
            store: FloorStore::new(redis),
            relay,
            tot_duration,
        }))
    }

    /// Registers the socket and announces it. On error the socket stays registered; the caller
    /// must still call [`Session::disconnect`].
    pub async fn connect(
        &self,
        group: &str,
        user: UserSession,
        tx: ClientTx,
    ) -> (Session, Result<(), StoreError>) {
        let socket_id: Arc<str> = uuid::Uuid::new_v4().to_string().into();
        let group: Arc<str> = group.into();
        let user_id: Arc<str> = user.sub.as_str().into();
        let client = Client {
            id: Arc::clone(&socket_id),
            user_id: Arc::clone(&user_id),
            tx: tx.clone(),
        };
        let room = self.0.registry.register(&group, client);
        let session = Session {
            hub: self.clone(),
            group,
            socket_id,
            user_id,
            user,
            room,
            tx,
        };
        let result = session.announce().await;
        (session, result)
    }

    async fn tot_expired(
        &self,
        group: Arc<str>,
        user_id: Arc<str>,
        socket_id: Arc<str>,
        generation: u64,
    ) {
        let Some(room) = self.0.registry.room(&group) else {
            return;
        };
        if !room.expire_tot(generation) {
            return;
        }
        tracing::warn!("[PTT] TOT expired: 30s limit exceeded for user {user_id} in group {group}");
        if let Some(client) = room.client(&socket_id) {
            client.send_text(
                Frame::FloorRevoked {
                    reason: RevokeReason::TimeOutTimerExpired,
                    preempted_by: None,
                }
                .to_text(),
            );
        }
        if let Err(err) = self.become_idle(&group, &user_id).await {
            tracing::error!(error = %err, "[PTT] could not publish idle state after TOT");
        }
    }

    /// Releases the Redis floor and tells everyone the channel is idle.
    async fn become_idle(&self, group: &str, user_id: &str) -> Result<(), StoreError> {
        self.0.store.release_floor(group, user_id).await?;
        let member_count = self.0.store.member_count(group).await?;
        self.broadcast(
            group,
            &Frame::FloorStateChanged {
                state: FloorState::Idle,
                speaker: None,
                member_count,
            },
            None,
        );
        Ok(())
    }

    /// Local fan-out followed by the relay to other instances. The room is looked up afresh so a
    /// session that just emptied it still reaches sockets that joined since.
    fn broadcast(&self, group: &str, frame: &Frame<'_>, exclude: Option<&str>) {
        let text = frame.to_text();
        if let Some(room) = self.0.registry.room(group) {
            room.broadcast_text(&text, exclude);
        }
        self.0.relay.publish_json(group, text.as_str(), exclude);
    }
}

/// Identity taken from the verified access token. The role is never client-supplied.
#[derive(Debug, Clone)]
pub struct UserSession {
    pub sub: String,
    pub name: String,
    pub role: String,
}

impl UserSession {
    /// Applies the same defaults as the TypeScript controller: name `Guard`, role `GUARD`.
    pub fn new(sub: String, name: Option<String>, role: Option<String>) -> Self {
        Self {
            sub,
            name: name
                .filter(|n| !n.is_empty())
                .unwrap_or_else(|| "Guard".into()),
            role: role
                .filter(|r| !r.is_empty())
                .unwrap_or_else(|| "GUARD".into())
                .to_uppercase(),
        }
    }

    fn is_admin(&self) -> bool {
        self.role == "ADMIN"
    }
}

/// One connected socket.
#[derive(Debug)]
pub struct Session {
    hub: Hub,
    group: Arc<str>,
    socket_id: Arc<str>,
    user_id: Arc<str>,
    user: UserSession,
    room: Arc<Room>,
    tx: ClientTx,
}

impl Session {
    async fn announce(&self) -> Result<(), StoreError> {
        let inner = &self.hub.0;
        inner.relay.join(&self.group).await?;
        let member_count = inner.store.add_member(&self.group, &self.user.sub).await?;
        let current = inner.store.current_floor(&self.group).await?;
        let state = if current.is_some() {
            FloorState::Transmitting
        } else {
            FloorState::Idle
        };
        let speaker = current.map(|c| Speaker {
            user_id: c.user_id,
            name: c.name,
            role: c.role,
        });
        self.tx.send_text(
            Frame::FloorStateChanged {
                state,
                speaker: speaker.as_ref(),
                member_count,
            }
            .to_text(),
        );
        self.hub.broadcast(
            &self.group,
            &Frame::MemberCountUpdated { member_count },
            Some(&self.socket_id),
        );
        tracing::info!(
            "[PTT] Connected: user {} ({}/{}) joined group {}. Members: {member_count}",
            self.user.sub,
            self.user.name,
            self.user.role,
            self.group
        );
        Ok(())
    }

    /// Handles a text frame.
    ///
    /// # Errors
    /// Redis failures while acting on the frame.
    pub async fn on_text(&self, text: &str) -> Result<(), StoreError> {
        if text.starts_with('\u{10}') {
            // Binary audio that arrived as a text frame: one latin-1 byte per character.
            #[expect(
                clippy::cast_possible_truncation,
                reason = "latin-1 keeps the low byte"
            )]
            let bytes: Vec<u8> = text.chars().map(|c| c as u32 as u8).collect();
            self.on_binary(&Bytes::from(bytes));
            return Ok(());
        }
        match parse_inbound(text) {
            None => tracing::warn!("[PTT] Malformed control frame received from socket"),
            Some(Inbound::FloorRequest { request_id }) => {
                self.floor_request(request_id.as_ref()).await?;
            }
            Some(Inbound::FloorReleased) => self.floor_released().await?,
            Some(Inbound::Unknown) => {}
        }
        Ok(())
    }

    /// Relays an Opus frame (7-byte header) from the active speaker to everyone else, locally and
    /// through the relay. Frames from anyone else, including a speaker preempted a moment ago,
    /// are dropped.
    pub fn on_binary(&self, frame: &Bytes) {
        let Some(renew_lease) = self.room.audio_gate(&self.socket_id, &self.user_id) else {
            return;
        };
        if renew_lease {
            let hub = self.hub.clone();
            let group = Arc::clone(&self.group);
            let user_id = Arc::clone(&self.user_id);
            tokio::spawn(async move {
                if let Err(err) = hub
                    .0
                    .store
                    .extend_lease(&group, &user_id, LEASE_SECONDS)
                    .await
                {
                    tracing::warn!(error = %err, "[PTT] failed to extend floor lease for user {user_id}");
                }
            });
        }
        self.room.broadcast_binary(frame, Some(&self.socket_id));
        self.hub.0.relay.publish_binary(&self.room, frame);
    }

    async fn floor_request(&self, request_id: Option<&Value>) -> Result<(), StoreError> {
        let inner = &self.hub.0;
        let lock = FloorLock::new(
            &self.user.sub,
            &self.user.name,
            &self.user.role,
            &self.socket_id,
        );
        let acquired = inner
            .store
            .acquire_floor(&self.group, &lock, LEASE_SECONDS)
            .await?;

        if !acquired.granted {
            let current = acquired.current_speaker.map(|s| Speaker {
                user_id: s.user_id,
                name: s.name,
                role: s.role,
            });
            let reason = if !self.user.is_admin()
                && current
                    .as_ref()
                    .is_some_and(|c| c.role.eq_ignore_ascii_case("ADMIN"))
            {
                DenyReason::InsufficientPriority
            } else {
                DenyReason::ChannelBusy
            };
            self.tx.send_text(
                Frame::FloorDenied {
                    request_id,
                    reason,
                    current_speaker: current.as_ref(),
                }
                .to_text(),
            );
            tracing::info!(
                "[PTT] Floor denied to user {} in group {}: {reason:?}",
                self.user.sub,
                self.group
            );
            return Ok(());
        }

        let hub = self.hub.clone();
        let (group, user_id, socket_id) = (
            Arc::clone(&self.group),
            Arc::clone(&self.user_id),
            Arc::clone(&self.socket_id),
        );
        self.room.grant(
            ActiveSpeaker {
                socket_id: Arc::clone(&self.socket_id),
                user_id: Arc::clone(&self.user_id),
            },
            |generation| {
                let tot = inner.tot_duration;
                tokio::spawn(async move {
                    tokio::time::sleep(tot).await;
                    hub.tot_expired(group, user_id, socket_id, generation).await;
                })
                .abort_handle()
            },
        );

        self.tx.send_text(
            Frame::FloorGranted {
                request_id,
                lease_ttl_ms: LEASE_TTL_MS,
            }
            .to_text(),
        );

        if let (true, Some(previous)) = (acquired.preempted, acquired.previous_speaker) {
            let revoked = Frame::FloorRevoked {
                reason: RevokeReason::AdminOverride,
                preempted_by: Some(self.user.name.as_str()),
            };
            // The previous speaker may be connected to another instance.
            inner
                .relay
                .publish_preempted(&self.group, &previous.user_id, &revoked);
            if let Some(client) = self.room.client_by_user(&previous.user_id) {
                client.send_text(revoked.to_text());
            }
            self.hub.broadcast(
                &self.group,
                &Frame::FloorReset {
                    reason: ResetReason::PreemptionFlush,
                },
                Some(&self.socket_id),
            );
            tracing::info!(
                "[PTT] Admin {} ({}) preempted guard {} in group {}",
                self.user.sub,
                self.user.name,
                previous.user_id,
                self.group
            );
        }

        let member_count = inner.store.member_count(&self.group).await?;
        let speaker = Speaker {
            user_id: self.user.sub.clone(),
            name: self.user.name.clone(),
            role: self.user.role.clone(),
        };
        self.hub.broadcast(
            &self.group,
            &Frame::FloorStateChanged {
                state: FloorState::Transmitting,
                speaker: Some(&speaker),
                member_count,
            },
            None,
        );
        Ok(())
    }

    async fn floor_released(&self) -> Result<(), StoreError> {
        if !self
            .room
            .is_active_speaker(&self.socket_id, Some(&self.user_id))
        {
            return Ok(());
        }
        self.room.clear_floor();
        self.hub.become_idle(&self.group, &self.user_id).await?;
        tracing::info!(
            "[PTT] Floor released by user {} in group {}. State is now IDLE.",
            self.user.sub,
            self.group
        );
        Ok(())
    }

    /// Removes the socket and tells the rest of the room; frees the floor if this socket held it.
    /// Always runs to the end of the local clean-up even when Redis fails.
    pub async fn disconnect(self) {
        let was_speaker = self.room.is_active_speaker(&self.socket_id, None);
        let inner = &self.hub.0;
        if inner.registry.unregister(&self.group, &self.socket_id) == 0 {
            inner.relay.leave(&self.group);
        }
        if let Err(err) = self.announce_departure(was_speaker).await {
            tracing::error!(error = %err, "[PTT] error during WebSocket close lifecycle");
        }
    }

    async fn announce_departure(&self, was_speaker: bool) -> Result<(), StoreError> {
        let inner = &self.hub.0;
        let member_count = inner
            .store
            .remove_member(&self.group, &self.user.sub)
            .await?;
        if was_speaker {
            self.room.clear_floor();
            inner
                .store
                .release_floor(&self.group, &self.user.sub)
                .await?;
            self.hub.broadcast(
                &self.group,
                &Frame::FloorStateChanged {
                    state: FloorState::Idle,
                    speaker: None,
                    member_count,
                },
                None,
            );
            tracing::info!(
                "[PTT] Active speaker disconnected. Floor reset to IDLE in group {}.",
                self.group
            );
        } else {
            self.hub.broadcast(
                &self.group,
                &Frame::MemberCountUpdated { member_count },
                None,
            );
        }
        tracing::info!(
            "[PTT] Disconnected: user {} left group {}. Remaining members: {member_count}",
            self.user.sub,
            self.group
        );
        Ok(())
    }

    pub fn close(&self, code: u16, reason: &'static str) {
        self.tx
            .send(Message::Close(Some(axum::extract::ws::CloseFrame {
                code,
                reason: reason.into(),
            })));
    }
}
