//! Per-process registry of connected sockets and the local view of each room's floor.
//!
//! Fan-out cost matters most here. A connection keeps an `Arc<Room>` from registration on, so
//! audio from a client never touches the room map; within a room, fan-out takes a read lock for
//! the few microseconds it needs to hand a refcounted frame to every listener's queue.

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, RwLock};
use std::time::{Duration, Instant};

use axum::extract::ws::{Message, Utf8Bytes};
use bytes::Bytes;
use dashmap::DashMap;
use dashmap::mapref::entry::Entry;
use tokio::sync::mpsc;
use tokio::task::AbortHandle;

use super::recorder::TurnTx;

/// A listener whose queue holds more than this is skipped for audio, which keeps latency bounded
/// instead of letting a slow link fall seconds behind (about five Opus frames).
pub const MAX_BUFFERED_BYTES: usize = 5 * 1024;

/// How often the Redis floor lease is renewed while audio flows.
const LEASE_RENEW_INTERVAL: Duration = Duration::from_secs(1);

/// Sending half of one connection's outbound queue, tracking the bytes still waiting to be
/// written so audio can be dropped under backpressure.
#[derive(Debug, Clone)]
pub struct ClientTx {
    tx: mpsc::UnboundedSender<Message>,
    queued: Arc<AtomicUsize>,
}

/// Receiving half, drained by the connection's writer task.
#[derive(Debug)]
pub struct Outbox {
    pub rx: mpsc::UnboundedReceiver<Message>,
    queued: Arc<AtomicUsize>,
}

impl Outbox {
    /// Call once a message of `len` payload bytes (see [`payload_len`]) has been handed to the
    /// socket.
    pub fn written(&self, len: usize) {
        self.queued.fetch_sub(len, Ordering::Relaxed);
    }
}

pub fn payload_len(message: &Message) -> usize {
    match message {
        Message::Text(text) => text.len(),
        Message::Binary(data) => data.len(),
        _ => 0,
    }
}

impl ClientTx {
    pub fn channel() -> (Self, Outbox) {
        let (tx, rx) = mpsc::unbounded_channel();
        let queued = Arc::new(AtomicUsize::new(0));
        (
            Self {
                tx,
                queued: Arc::clone(&queued),
            },
            Outbox { rx, queued },
        )
    }

    /// `false` when the connection is gone.
    pub fn send(&self, message: Message) -> bool {
        let len = payload_len(&message);
        self.queued.fetch_add(len, Ordering::Relaxed);
        if self.tx.send(message).is_err() {
            self.queued.fetch_sub(len, Ordering::Relaxed);
            return false;
        }
        true
    }

    pub fn send_text(&self, text: Utf8Bytes) -> bool {
        self.send(Message::Text(text))
    }

    fn queued(&self) -> usize {
        self.queued.load(Ordering::Relaxed)
    }
}

#[derive(Debug, Clone)]
pub struct Client {
    pub id: Arc<str>,
    pub user_id: Arc<str>,
    pub tx: ClientTx,
}

/// Who currently holds this room's floor, as far as this process knows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveSpeaker {
    pub socket_id: Arc<str>,
    pub user_id: Arc<str>,
}

#[derive(Debug)]
struct Tot {
    generation: u64,
    task: AbortHandle,
}

#[derive(Debug, Default)]
struct Floor {
    speaker: Option<ActiveSpeaker>,
    tot: Option<Tot>,
    last_lease_renewal: Option<Instant>,
    /// Ends the current turn's recording when dropped, which is why every way the floor can
    /// change (release, time-out, preemption, disconnect, a new grant) goes through here.
    recorder: Option<TurnTx>,
}

impl Floor {
    fn clear(&mut self) {
        self.speaker = None;
        self.recorder = None;
        self.last_lease_renewal = None;
        if let Some(tot) = self.tot.take() {
            tot.task.abort();
        }
    }
}

/// What the room decided about one inbound audio frame, see [`Room::audio_gate`].
#[derive(Debug)]
pub struct AudioGate {
    pub renew_lease: bool,
    pub recorder: Option<TurnTx>,
}

#[derive(Debug)]
pub struct Room {
    clients: RwLock<Vec<Client>>,
    floor: Mutex<Floor>,
    next_generation: AtomicU64,
    pub audio_channel: Arc<str>,
}

impl Room {
    fn new(group: &str) -> Self {
        Self {
            clients: RwLock::new(Vec::new()),
            floor: Mutex::new(Floor::default()),
            next_generation: AtomicU64::new(0),
            audio_channel: format!("ptt:room:{group}:audio").into(),
        }
    }

    fn floor(&self) -> MutexGuard<'_, Floor> {
        self.floor.lock().expect("floor lock poisoned")
    }

    pub fn client_count(&self) -> usize {
        self.clients.read().expect("clients lock poisoned").len()
    }

    pub fn client(&self, socket_id: &str) -> Option<ClientTx> {
        let clients = self.clients.read().expect("clients lock poisoned");
        clients
            .iter()
            .find(|c| &*c.id == socket_id)
            .map(|c| c.tx.clone())
    }

    /// The first socket of `user_id` in connection order.
    pub fn client_by_user(&self, user_id: &str) -> Option<ClientTx> {
        let clients = self.clients.read().expect("clients lock poisoned");
        clients
            .iter()
            .find(|c| &*c.user_id == user_id)
            .map(|c| c.tx.clone())
    }

    pub fn broadcast_text(&self, text: &Utf8Bytes, exclude: Option<&str>) {
        let clients = self.clients.read().expect("clients lock poisoned");
        for client in clients.iter().filter(|c| Some(&*c.id) != exclude) {
            client.tx.send_text(text.clone());
        }
    }

    /// Hands `frame` to every listener except `exclude`, skipping those with a full queue.
    /// Returns `(sent, dropped)`.
    pub fn broadcast_binary(&self, frame: &Bytes, exclude: Option<&str>) -> (usize, usize) {
        let clients = self.clients.read().expect("clients lock poisoned");
        let (mut sent, mut dropped) = (0, 0);
        for client in clients.iter().filter(|c| Some(&*c.id) != exclude) {
            if client.tx.queued() > MAX_BUFFERED_BYTES
                || !client.tx.send(Message::Binary(frame.clone()))
            {
                dropped += 1;
            } else {
                sent += 1;
            }
        }
        (sent, dropped)
    }

    /// The socket matches, or `user_id` does (another socket of the same user also counts, as in
    /// the TypeScript server).
    pub fn is_active_speaker(&self, socket_id: &str, user_id: Option<&str>) -> bool {
        self.floor().speaker.as_ref().is_some_and(|current| {
            &*current.socket_id == socket_id || user_id.is_some_and(|u| &*current.user_id == u)
        })
    }

    /// Decides whether an inbound audio frame may be relayed. `None` drops it; otherwise the gate
    /// says whether the Redis lease is due for renewal (at most once per second) and, when the
    /// turn is being recorded, where to send the frame. Taken under the same lock as the speaker
    /// check so a frame can never reach the next speaker's recording.
    pub fn audio_gate(&self, socket_id: &str, user_id: &str) -> Option<AudioGate> {
        let mut floor = self.floor();
        let current = floor.speaker.as_ref()?;
        if &*current.socket_id != socket_id && &*current.user_id != user_id {
            return None;
        }
        let now = Instant::now();
        let due = floor
            .last_lease_renewal
            .is_none_or(|last| now.duration_since(last) >= LEASE_RENEW_INTERVAL);
        if due {
            floor.last_lease_renewal = Some(now);
        }
        Some(AudioGate {
            renew_lease: due,
            recorder: floor.recorder.clone(),
        })
    }

    #[cfg(test)]
    pub fn active_speaker(&self) -> Option<ActiveSpeaker> {
        self.floor().speaker.clone()
    }

    /// Makes `speaker` the active speaker and arms the time-out timer, replacing any previous one.
    /// `arm` receives the timer's generation and returns the handle that cancels it. `recorder`
    /// replaces the previous turn's, which ends that recording.
    pub fn grant(
        &self,
        speaker: ActiveSpeaker,
        recorder: Option<TurnTx>,
        arm: impl FnOnce(u64) -> AbortHandle,
    ) {
        let generation = self.next_generation.fetch_add(1, Ordering::Relaxed);
        let mut floor = self.floor();
        floor.speaker = Some(speaker);
        floor.recorder = recorder;
        let previous = floor.tot.replace(Tot {
            generation,
            task: arm(generation),
        });
        if let Some(previous) = previous {
            previous.task.abort();
        }
    }

    /// Back to idle: no speaker, no timer, lease throttle reset.
    pub fn clear_floor(&self) {
        self.floor().clear();
    }

    /// Called by the timer task. Clears the floor and returns `true` only if this timer is still
    /// the current one; the abort in [`Room::grant`] cannot stop a task that already woke up.
    pub fn expire_tot(&self, generation: u64) -> bool {
        let mut floor = self.floor();
        if floor.tot.as_ref().map(|t| t.generation) != Some(generation) {
            return false;
        }
        floor.tot = None;
        floor.clear();
        true
    }

    /// Another instance preempted `previous_user_id`; if that user holds the floor here, clears it
    /// and returns the speaker's socket.
    pub fn clear_if_speaker(&self, previous_user_id: &str) -> Option<Arc<str>> {
        let mut floor = self.floor();
        let socket_id = match &floor.speaker {
            Some(speaker) if &*speaker.user_id == previous_user_id => {
                Arc::clone(&speaker.socket_id)
            }
            _ => return None,
        };
        floor.clear();
        Some(socket_id)
    }
}

#[derive(Debug, Default)]
pub struct Registry {
    rooms: DashMap<Arc<str>, Arc<Room>>,
}

impl Registry {
    pub fn register(&self, group: &str, client: Client) -> Arc<Room> {
        // The shard lock is held until `entry` drops, so a concurrent `unregister` cannot remove
        // the room between creating it and adding the client.
        let entry = self
            .rooms
            .entry(group.into())
            .or_insert_with(|| Arc::new(Room::new(group)));
        let room = Arc::clone(entry.value());
        room.clients
            .write()
            .expect("clients lock poisoned")
            .push(client);
        room
    }

    /// Removes the socket and returns how many remain. An emptied room is dropped along with its
    /// speaker and timer.
    pub fn unregister(&self, group: &str, socket_id: &str) -> usize {
        let Entry::Occupied(entry) = self.rooms.entry(group.into()) else {
            return 0;
        };
        let room = entry.get();
        let remaining = {
            let mut clients = room.clients.write().expect("clients lock poisoned");
            clients.retain(|c| &*c.id != socket_id);
            clients.len()
        };
        if remaining == 0 {
            room.clear_floor();
            entry.remove();
        }
        remaining
    }

    pub fn room(&self, group: &str) -> Option<Arc<Room>> {
        self.rooms.get(group).map(|r| Arc::clone(r.value()))
    }

    pub fn client_count(&self, group: &str) -> usize {
        self.room(group).map_or(0, |room| room.client_count())
    }
}

#[cfg(test)]
mod tests {
    use tokio::sync::mpsc::error::TryRecvError;

    use super::*;

    fn client(id: &str, user: &str) -> (Client, Outbox) {
        let (tx, outbox) = ClientTx::channel();
        (
            Client {
                id: id.into(),
                user_id: user.into(),
                tx,
            },
            outbox,
        )
    }

    fn speaker(socket: &str, user: &str) -> ActiveSpeaker {
        ActiveSpeaker {
            socket_id: socket.into(),
            user_id: user.into(),
        }
    }

    fn noop_task(_generation: u64) -> AbortHandle {
        tokio::spawn(std::future::pending::<()>()).abort_handle()
    }

    #[tokio::test]
    async fn register_and_unregister_drop_empty_rooms() {
        let registry = Registry::default();
        let (a, _oa) = client("s1", "u1");
        let (b, _ob) = client("s2", "u2");
        registry.register("g", a);
        registry.register("g", b);
        assert_eq!(registry.client_count("g"), 2);
        assert_eq!(registry.unregister("g", "s1"), 1);
        assert!(registry.room("g").is_some());
        assert_eq!(registry.unregister("g", "s2"), 0);
        assert!(registry.room("g").is_none());
        assert_eq!(registry.unregister("g", "missing"), 0);
    }

    #[tokio::test]
    async fn emptying_a_room_clears_its_speaker() {
        let registry = Registry::default();
        let (a, _oa) = client("s1", "u1");
        let room = registry.register("g", a);
        room.grant(speaker("s1", "u1"), None, noop_task);
        registry.unregister("g", "s1");
        assert_eq!(room.active_speaker(), None);
    }

    #[tokio::test]
    async fn broadcast_text_skips_the_excluded_socket() {
        let registry = Registry::default();
        let (a, mut oa) = client("s1", "u1");
        let (b, mut ob) = client("s2", "u2");
        let room = registry.register("g", a);
        registry.register("g", b);
        room.broadcast_text(&"hi".into(), Some("s1"));
        assert!(oa.rx.try_recv().is_err());
        assert!(matches!(ob.rx.try_recv(), Ok(Message::Text(t)) if t.as_str() == "hi"));
    }

    #[tokio::test]
    async fn binary_is_dropped_for_backed_up_listeners_until_they_drain() {
        let registry = Registry::default();
        let (a, _oa) = client("s1", "u1");
        let (b, mut ob) = client("s2", "u2");
        let room = registry.register("g", a);
        registry.register("g", b);

        let frame = Bytes::from(vec![0_u8; 2048]);
        assert_eq!(room.broadcast_binary(&frame, Some("s1")), (1, 0));
        assert_eq!(room.broadcast_binary(&frame, Some("s1")), (1, 0));
        assert_eq!(room.broadcast_binary(&frame, Some("s1")), (1, 0));
        // 6 KiB queued now exceeds the 5 KiB ceiling.
        assert_eq!(room.broadcast_binary(&frame, Some("s1")), (0, 1));

        let first = ob.rx.try_recv().expect("queued frame");
        ob.written(payload_len(&first));
        let second = ob.rx.try_recv().expect("queued frame");
        ob.written(payload_len(&second));
        assert_eq!(room.broadcast_binary(&frame, Some("s1")), (1, 0));
    }

    #[tokio::test]
    async fn broadcast_binary_reports_dead_connections_as_dropped() {
        let registry = Registry::default();
        let (a, oa) = client("s1", "u1");
        let room = registry.register("g", a);
        drop(oa);
        assert_eq!(
            room.broadcast_binary(&Bytes::from_static(b"x"), None),
            (0, 1)
        );
    }

    #[tokio::test]
    async fn active_speaker_matches_by_socket_or_user() {
        let registry = Registry::default();
        let (a, _oa) = client("s1", "u1");
        let room = registry.register("g", a);
        assert!(!room.is_active_speaker("s1", Some("u1")));
        room.grant(speaker("s1", "u1"), None, noop_task);
        assert!(room.is_active_speaker("s1", None));
        assert!(room.is_active_speaker("other", Some("u1")));
        assert!(!room.is_active_speaker("other", Some("u2")));
        assert!(!room.is_active_speaker("other", None));
    }

    #[tokio::test]
    async fn audio_gate_throttles_lease_renewal_to_once_per_interval() {
        let registry = Registry::default();
        let (a, _oa) = client("s1", "u1");
        let room = registry.register("g", a);
        assert!(room.audio_gate("s1", "u1").is_none());
        room.grant(speaker("s1", "u1"), None, noop_task);
        let renew = |s: &str, u: &str| room.audio_gate(s, u).map(|gate| gate.renew_lease);
        assert_eq!(renew("s1", "u1"), Some(true));
        assert_eq!(renew("s1", "u1"), Some(false));
        assert_eq!(renew("zz", "zz"), None);
        room.clear_floor();
        assert!(room.audio_gate("s1", "u1").is_none());
    }

    #[tokio::test]
    async fn regranting_makes_the_old_timer_stale() {
        let registry = Registry::default();
        let (a, _oa) = client("s1", "u1");
        let room = registry.register("g", a);
        let mut generations = Vec::new();
        for _ in 0..2 {
            room.grant(speaker("s1", "u1"), None, |generation| {
                generations.push(generation);
                noop_task(generation)
            });
        }
        assert!(!room.expire_tot(generations[0]));
        assert!(room.active_speaker().is_some());
        assert!(room.expire_tot(generations[1]));
        assert_eq!(room.active_speaker(), None);
        assert!(!room.expire_tot(generations[1]));
    }

    #[tokio::test]
    async fn clear_if_speaker_only_matches_the_previous_user() {
        let registry = Registry::default();
        let (a, _oa) = client("s1", "u1");
        let room = registry.register("g", a);
        room.grant(speaker("s1", "u1"), None, noop_task);
        assert_eq!(room.clear_if_speaker("u2"), None);
        assert_eq!(room.clear_if_speaker("u1").as_deref(), Some("s1"));
        assert_eq!(room.active_speaker(), None);
    }

    #[tokio::test]
    async fn every_way_the_floor_changes_ends_the_recording() {
        use tokio::sync::mpsc::error::TryRecvError::Disconnected;

        let registry = Registry::default();
        let (a, _oa) = client("s1", "u1");
        let room = registry.register("g", a);

        let (first, mut first_rx) = TurnTx::detached(4);
        room.grant(speaker("s1", "u1"), Some(first), noop_task);
        let gate = room.audio_gate("s1", "u1").expect("speaker");
        assert!(gate.recorder.is_some());
        drop(gate);
        assert!(matches!(first_rx.try_recv(), Err(TryRecvError::Empty)));

        // A new grant (preemption) replaces the recorder and ends the old turn.
        let (second, mut second_rx) = TurnTx::detached(4);
        room.grant(speaker("s2", "u2"), Some(second), noop_task);
        assert!(matches!(first_rx.try_recv(), Err(Disconnected)));
        assert!(room.audio_gate("s1", "u1").is_none());

        room.clear_floor();
        assert!(matches!(second_rx.try_recv(), Err(Disconnected)));

        let (third, mut third_rx) = TurnTx::detached(4);
        room.grant(speaker("s1", "u1"), Some(third), noop_task);
        registry.unregister("g", "s1");
        assert!(matches!(third_rx.try_recv(), Err(Disconnected)));
    }
}
