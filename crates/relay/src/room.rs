//! Seats — each pairs one client with one **daemon instance**, and outlives the client's socket.
//!
//! Keyed by instance id, not by Remote ID: a Remote ID names a pool of daemons (see
//! [`crate::registry`]) and each of them can hold a client of its own. That is what lets two
//! devices use one Remote ID at the same time.
//!
//! A seat has a client half that is either **live** (its socket is open) or **held** (the
//! socket dropped, and the seat is kept for that same client — by `client_key` — for the
//! daemon's hold time, which is the daemon's own grace for the desktop behind it). A held seat
//! is what stops a phone that lost signal for a minute from coming back to find another device
//! on its desktop. The daemon half is not stored here at all: a daemon that drops simply
//! leaves the registry, and its held seats wait for it to come back under the same instance id.
//!
//! **Nothing here takes a seat from a live client on its own.** Two devices that both want one
//! instance and both reconnect when their socket closes will evict each other forever —
//! measured on `2026-09-14` at 132 joins in two minutes, the failure `issues.md` I17 recorded at
//! a slower 18 s period. The one exception is `takeover`, which a client sends only after a
//! human tapped "use it here", and the client it displaces is told so and does not retry.

use std::sync::Arc;
use std::time::{Duration, Instant};

use dashmap::mapref::entry::Entry;
use dashmap::DashMap;
use tokio::sync::{mpsc, oneshot};
use wado_protocol::relay_wire::WireMsg;

/// One seat.
struct Seat {
    room_id: String,
    client_key: String,
    client_name: String,
    /// `Some` while the client's socket is open.
    link: Option<Link>,
    /// Set when the client dropped: the seat is its until then.
    held_until: Option<Instant>,
    hold: Duration,
    /// The daemon's `peer_accept` / `peer_reject`, while a gated join waits for one.
    verdict: Option<oneshot::Sender<Result<(), String>>>,
    /// The daemon let this client in. Until then nothing the daemon sends is forwarded to it:
    /// a device waiting for approval must not read the daemon's traffic.
    accepted: bool,
}

struct Link {
    inbox_tx: mpsc::Sender<String>,
    /// Dropped with the link; the client's loop ends when it is.
    _kick: oneshot::Sender<()>,
}

impl Seat {
    fn expired(&self, now: Instant) -> bool {
        self.held_until.is_some_and(|t| now >= t)
    }
    /// Live, or held and not yet expired.
    fn taken(&self, now: Instant) -> bool {
        self.link.is_some() || !self.expired(now)
    }
}

/// Who wants a seat.
pub struct Want {
    pub room_id: String,
    pub client_key: String,
    pub client_name: String,
    pub inbox_tx: mpsc::Sender<String>,
    pub hold: Duration,
    /// A human asked for this seat even if another device is on it.
    pub takeover: bool,
}

pub enum Claim {
    Got {
        /// Resolves when this client's link is dropped by someone else (a takeover).
        kicked: oneshot::Receiver<()>,
        /// The seat was being held for this client.
        reclaimed: bool,
        /// A takeover displaced a live client: its room id, so the daemon can be told.
        displaced: Option<String>,
    },
    Busy {
        holder: String,
        held: bool,
    },
}

/// At most one seat per daemon instance.
#[derive(Default, Clone)]
pub struct RoomStore {
    inner: Arc<DashMap<String, Seat>>,
    /// Takeover checks waiting for the daemon's verdict, by room id. No seat exists yet for
    /// these — the whole point is to ask before taking one.
    checks: Arc<DashMap<String, oneshot::Sender<Result<(), String>>>>,
}

impl RoomStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Take an instance's seat for this client, if the rules allow — see the module docs.
    pub fn claim(&self, instance_id: &str, w: Want) -> Claim {
        let now = Instant::now();
        let (kick, kicked) = oneshot::channel();
        let seat = |w: Want| Seat {
            room_id: w.room_id,
            client_key: w.client_key,
            client_name: w.client_name,
            link: Some(Link {
                inbox_tx: w.inbox_tx,
                _kick: kick,
            }),
            held_until: None,
            hold: w.hold,
            verdict: None,
            accepted: false,
        };
        match self.inner.entry(instance_id.to_string()) {
            Entry::Vacant(slot) => {
                slot.insert(seat(w));
                Claim::Got {
                    kicked,
                    reclaimed: false,
                    displaced: None,
                }
            }
            Entry::Occupied(mut slot) => {
                let s = slot.get();
                let live = s.link.is_some();
                let mine = !w.client_key.is_empty() && s.client_key == w.client_key;
                let expired = s.expired(now);
                let reclaimed = !live && !expired && mine;
                let displaced = if live && w.takeover {
                    // Told before it is dropped, so the message is queued ahead of the close.
                    let by = w.client_name.clone();
                    if let (Some(link), Ok(text)) =
                        (&s.link, serde_json::to_string(&WireMsg::TakenOver { by }))
                    {
                        let _ = link.inbox_tx.try_send(text);
                    }
                    Some(s.room_id.clone())
                } else {
                    None
                };
                if !live && (expired || mine || w.takeover) || displaced.is_some() {
                    slot.insert(seat(w)); // drops the old link → its loop ends
                    Claim::Got {
                        kicked,
                        reclaimed,
                        displaced,
                    }
                } else {
                    Claim::Busy {
                        holder: s.client_name.clone(),
                        held: !live,
                    }
                }
            }
        }
    }

    /// This client's held seat on `instance_id`, if it has one: time left on the hold.
    pub fn held_for(&self, instance_id: &str, client_key: &str) -> Option<Duration> {
        if client_key.is_empty() {
            return None;
        }
        let s = self.inner.get(instance_id)?;
        let t = s.held_until?;
        (s.link.is_none() && s.client_key == client_key)
            .then(|| t.saturating_duration_since(Instant::now()))
            .filter(|d| !d.is_zero())
    }

    /// Who holds `instance_id`, if anyone does.
    pub fn holder(&self, instance_id: &str) -> Option<String> {
        let s = self.inner.get(instance_id)?;
        s.taken(Instant::now()).then(|| s.client_name.clone())
    }

    /// The client's socket closed. Keep its seat for the hold time, or drop it when there is
    /// nothing to hold it for (an older client with no key, or a daemon that asked for none).
    /// Only acts if the seat is still this room's — a displaced client must not touch its
    /// successor's.
    pub fn release(&self, instance_id: &str, room_id: &str) {
        let removed = self.inner.remove_if(instance_id, |_, s| {
            s.room_id == room_id && (s.client_key.is_empty() || s.hold.is_zero())
        });
        if removed.is_none() {
            if let Some(mut s) = self.inner.get_mut(instance_id) {
                if s.room_id == room_id {
                    s.link = None;
                    s.verdict = None;
                    s.held_until = Some(Instant::now() + s.hold);
                }
            }
        }
    }

    /// The daemon's hold for this room's seat. Applies to a seat already held, too: a hold of
    /// zero frees it now, a shorter one brings its expiry forward.
    pub fn set_hold(&self, instance_id: &str, room_id: &str, hold: Duration) {
        let removed = self.inner.remove_if(instance_id, |_, s| {
            s.room_id == room_id && s.link.is_none() && hold.is_zero()
        });
        if removed.is_some() {
            tracing::info!(instance = %instance_id, "seat freed — the daemon has nothing to hold it for");
            return;
        }
        if let Some(mut s) = self.inner.get_mut(instance_id) {
            if s.room_id == room_id {
                s.hold = hold;
                if let Some(t) = s.held_until {
                    s.held_until = Some(t.min(Instant::now() + hold));
                }
            }
        }
    }

    /// Drop a seat outright (a gated join was refused).
    pub fn remove_if(&self, instance_id: &str, room_id: &str) {
        self.inner
            .remove_if(instance_id, |_, s| s.room_id == room_id);
    }

    /// Arrange for the daemon's verdict on this room to arrive on the returned receiver.
    pub fn await_verdict(
        &self,
        instance_id: &str,
        room_id: &str,
    ) -> Option<oneshot::Receiver<Result<(), String>>> {
        let mut s = self.inner.get_mut(instance_id)?;
        if s.room_id != room_id {
            return None;
        }
        let (tx, rx) = oneshot::channel();
        s.verdict = Some(tx);
        Some(rx)
    }

    /// Arrange for the daemon's answer to a takeover check to arrive on the returned receiver.
    pub fn await_check(&self, room_id: &str) -> oneshot::Receiver<Result<(), String>> {
        let (tx, rx) = oneshot::channel();
        self.checks.insert(room_id.to_string(), tx);
        rx
    }

    pub fn drop_check(&self, room_id: &str) {
        self.checks.remove(room_id);
    }

    /// The daemon answered a gated join or a takeover check.
    pub fn deliver_verdict(&self, instance_id: &str, room_id: &str, verdict: Result<(), String>) {
        if let Some((_, tx)) = self.checks.remove(room_id) {
            let _ = tx.send(verdict);
            return;
        }
        if let Some(mut s) = self.inner.get_mut(instance_id) {
            if s.room_id == room_id {
                if let Some(tx) = s.verdict.take() {
                    let _ = tx.send(verdict);
                }
            }
        }
    }

    /// The join on this room passed (or did not need) the daemon's gate.
    pub fn accept(&self, instance_id: &str, room_id: &str) {
        if let Some(mut s) = self.inner.get_mut(instance_id) {
            if s.room_id == room_id {
                s.accepted = true;
            }
        }
    }

    /// How many of `instances` are taken. For the occupancy the client is shown.
    pub fn busy_among(&self, instances: &[String]) -> usize {
        let now = Instant::now();
        instances
            .iter()
            .filter(|i| self.inner.get(*i).is_some_and(|s| s.taken(now)))
            .count()
    }

    /// Forward a JSON message to the client on `instance_id`'s seat.
    /// Returns false if there is no live client (none yet, or held while it is away).
    pub async fn forward_to_client(&self, instance_id: &str, msg: String) -> bool {
        // Cloned out of the map before the await: holding a DashMap reference across an await
        // point holds that shard's lock, and every other room on the shard blocks behind one
        // slow client's socket.
        let tx = self
            .inner
            .get(instance_id)
            .filter(|s| s.accepted)
            .and_then(|s| s.link.as_ref().map(|l| l.inbox_tx.clone()));
        match tx {
            Some(tx) => tx.send(msg).await.is_ok(),
            None => false,
        }
    }

    pub fn count(&self) -> usize {
        self.inner.len()
    }
}

#[cfg(test)]
mod tests {
    use super::{Claim, RoomStore, Want};
    use std::time::Duration;
    use tokio::sync::mpsc;

    fn want(room: &str, key: &str, takeover: bool) -> (Want, mpsc::Receiver<String>) {
        let (tx, rx) = mpsc::channel(4);
        (
            Want {
                room_id: room.into(),
                client_key: key.into(),
                client_name: format!("dev-{key}"),
                inbox_tx: tx,
                hold: Duration::from_secs(60),
                takeover,
            },
            rx,
        )
    }

    fn got(c: &Claim) -> bool {
        matches!(c, Claim::Got { .. })
    }

    #[test]
    fn a_dropped_client_keeps_its_seat_and_nobody_else_gets_it() {
        let rooms = RoomStore::new();
        let (a, _ra) = want("r1", "A", false);
        assert!(got(&rooms.claim("i", a)));
        rooms.release("i", "r1");
        assert!(rooms.held_for("i", "A").is_some());
        // Another device is refused — the seat is held, not free.
        let (b, _rb) = want("r2", "B", false);
        assert!(matches!(
            rooms.claim("i", b),
            Claim::Busy { held: true, .. }
        ));
        // The owner comes back and gets it.
        let (a2, _ra2) = want("r3", "A", false);
        assert!(matches!(
            rooms.claim("i", a2),
            Claim::Got {
                reclaimed: true,
                ..
            }
        ));
    }

    #[test]
    fn a_seat_the_daemon_has_nothing_to_hold_for_is_freed() {
        let rooms = RoomStore::new();
        let (a, _ra) = want("r1", "A", false);
        assert!(got(&rooms.claim("i", a)));
        rooms.release("i", "r1");
        rooms.set_hold("i", "r1", Duration::ZERO);
        let (b, _rb) = want("r2", "B", false);
        assert!(
            got(&rooms.claim("i", b)),
            "a ghost seat refused another device"
        );
    }

    #[test]
    fn a_live_seat_is_never_taken_without_a_tap_not_even_by_its_own_key() {
        let rooms = RoomStore::new();
        let (a, _ra) = want("r1", "A", false);
        assert!(got(&rooms.claim("i", a)));
        // Two tabs of one browser share a key; the second must not steal (I17).
        let (a2, _ra2) = want("r2", "A", false);
        assert!(matches!(
            rooms.claim("i", a2),
            Claim::Busy { held: false, .. }
        ));
    }

    #[test]
    fn a_takeover_moves_the_seat_and_tells_the_displaced_client() {
        let rooms = RoomStore::new();
        let (a, mut ra) = want("r1", "A", false);
        let Claim::Got { mut kicked, .. } = rooms.claim("i", a) else {
            panic!()
        };
        let (b, _rb) = want("r2", "B", true);
        match rooms.claim("i", b) {
            Claim::Got { displaced, .. } => assert_eq!(displaced.as_deref(), Some("r1")),
            Claim::Busy { .. } => panic!("takeover refused"),
        }
        assert!(ra.try_recv().unwrap().contains("taken_over"));
        assert!(kicked.try_recv().is_err()); // closed: the old loop ends
                                             // The displaced client's cleanup must not free its successor's seat.
        rooms.release("i", "r1");
        assert_eq!(rooms.holder("i").as_deref(), Some("dev-B"));
    }

    #[test]
    fn a_hold_expires() {
        let rooms = RoomStore::new();
        let (mut a, _ra) = want("r1", "A", false);
        a.hold = Duration::ZERO;
        assert!(got(&rooms.claim("i", a)));
        rooms.release("i", "r1"); // zero hold: dropped outright
        assert_eq!(rooms.count(), 0);
        let (b, _rb) = want("r2", "B", false);
        assert!(got(&rooms.claim("i", b)));
    }

    #[test]
    fn keyless_clients_are_not_held() {
        let rooms = RoomStore::new();
        let (a, _ra) = want("r1", "", false);
        assert!(got(&rooms.claim("i", a)));
        rooms.release("i", "r1");
        assert_eq!(rooms.count(), 0);
    }
}
