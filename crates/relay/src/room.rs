//! Signaling room — pairs one registered server with one connected client.
//!
//! One room per Remote ID. If a second client tries to join while a room is
//! active, the join is rejected ("room full") — one viewer at a time (iter 1).
//! The room is destroyed when either WS closes.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;

use dashmap::DashMap;
use tokio::sync::mpsc;

/// An active signaling room.
#[allow(dead_code)]
pub struct SignalingRoom {
    pub room_id: String,
    pub remote_id: String,
    pub client_addr: SocketAddr,
    /// Send a JSON string here to deliver it to the connected client's WS.
    pub client_inbox_tx: mpsc::Sender<String>,
    pub created_at: Instant,
}

/// Stores at most one room per Remote ID.
#[derive(Default, Clone)]
pub struct RoomStore {
    inner: Arc<DashMap<String, SignalingRoom>>,
}

impl RoomStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a room. Returns `Err` if a room already exists for this Remote ID.
    pub fn create(
        &self,
        remote_id: String,
        room_id: String,
        client_addr: SocketAddr,
        client_inbox_tx: mpsc::Sender<String>,
    ) -> Result<(), String> {
        if self.inner.contains_key(&remote_id) {
            return Err(format!("Remote ID '{remote_id}' already has an active room"));
        }
        self.inner.insert(
            remote_id.clone(),
            SignalingRoom { room_id, remote_id, client_addr, client_inbox_tx, created_at: Instant::now() },
        );
        Ok(())
    }

    /// Remove the room for a Remote ID (called when either side disconnects).
    pub fn remove(&self, remote_id: &str) {
        self.inner.remove(remote_id);
    }

    /// Forward a JSON message to the client paired with `remote_id`.
    /// Returns false if no room exists (client already gone).
    pub async fn forward_to_client(&self, remote_id: &str, msg: String) -> bool {
        if let Some(room) = self.inner.get(remote_id) {
            room.client_inbox_tx.send(msg).await.is_ok()
        } else {
            false
        }
    }

    pub fn count(&self) -> usize {
        self.inner.len()
    }
}
