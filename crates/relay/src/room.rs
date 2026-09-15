//! Signaling rooms — each pairs one client with one **daemon instance**.
//!
//! Keyed by instance id, not by Remote ID: a Remote ID names a pool of daemons (see
//! [`crate::registry`]) and each of them can hold a client of its own. That is what lets two
//! devices use one Remote ID at the same time.
//!
//! A room is destroyed when its client's WS closes, or when its daemon disconnects. It is
//! never taken from a live client — see [`RoomStore::claim`].

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;

use dashmap::DashMap;
use tokio::sync::mpsc;

/// An active signaling room.
#[allow(dead_code)]
pub struct SignalingRoom {
    pub room_id: String,
    pub instance_id: String,
    pub remote_id: String,
    pub client_addr: SocketAddr,
    /// Send a JSON string here to deliver it to the connected client's WS.
    pub client_inbox_tx: mpsc::Sender<String>,
    pub created_at: Instant,
}

/// At most one room per daemon instance.
#[derive(Default, Clone)]
pub struct RoomStore {
    inner: Arc<DashMap<String, SignalingRoom>>,
}

impl RoomStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Take an instance for this client, but only if it is free.
    ///
    /// Returns `false` when the instance already has a client. **Nothing in this file ever
    /// takes an occupied instance**, and that is the whole design: two devices that both want
    /// one instance and both reconnect when their socket closes will evict each other forever.
    /// Measured here on `2026-09-14` at 132 joins in two minutes, each evicting the last — the
    /// same failure `issues.md` I17 recorded at a slower 18 s period. Contention is answered
    /// with a *different* daemon, or with a plain refusal. Never by stealing.
    pub fn claim(
        &self,
        instance_id: &str,
        remote_id: &str,
        room_id: String,
        client_addr: SocketAddr,
        client_inbox_tx: mpsc::Sender<String>,
    ) -> bool {
        use dashmap::mapref::entry::Entry;
        match self.inner.entry(instance_id.to_string()) {
            Entry::Occupied(_) => false,
            Entry::Vacant(slot) => {
                slot.insert(SignalingRoom {
                    room_id,
                    instance_id: instance_id.to_string(),
                    remote_id: remote_id.to_string(),
                    client_addr,
                    client_inbox_tx,
                    created_at: Instant::now(),
                });
                true
            }
        }
    }

    /// Whether an instance currently has a client.
    pub fn is_busy(&self, instance_id: &str) -> bool {
        self.inner.contains_key(instance_id)
    }

    /// How many of `instances` are occupied. For the occupancy the client is shown.
    pub fn busy_among(&self, instances: &[String]) -> usize {
        instances.iter().filter(|i| self.inner.contains_key(*i)).count()
    }

    /// Drop the room for an instance whatever it holds (the daemon itself went away).
    pub fn remove(&self, instance_id: &str) {
        self.inner.remove(instance_id);
    }

    /// Remove an instance's room **only if** it is still the one identified by `room_id`.
    ///
    /// A client that was displaced by its own reconnect runs cleanup on the way out, and an
    /// unconditional remove there would delete the room belonging to the connection that
    /// replaced it — leaving the newcomer with an open socket and nothing routed to it.
    pub fn remove_if(&self, instance_id: &str, room_id: &str) {
        self.inner.remove_if(instance_id, |_, room| room.room_id == room_id);
    }

    /// Forward a JSON message to the client paired with `instance_id`.
    /// Returns false if no room exists (client already gone).
    pub async fn forward_to_client(&self, instance_id: &str, msg: String) -> bool {
        // Cloned out of the map before the await: holding a DashMap reference across an await
        // point holds that shard's lock, and every other room on the shard blocks behind one
        // slow client's socket.
        let tx = self.inner.get(instance_id).map(|room| room.client_inbox_tx.clone());
        match tx {
            Some(tx) => tx.send(msg).await.is_ok(),
            None => false,
        }
    }

    pub fn count(&self) -> usize {
        self.inner.len()
    }
}
