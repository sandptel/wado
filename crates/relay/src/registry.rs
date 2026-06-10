//! Server registry — tracks connected wado-server instances, keyed by Remote ID.
//!
//! Each server opens a persistent WS to `/register` and stores its entry here.
//! On WS disconnect the entry is removed. All access is lock-free via DashMap.
//! Keys are **normalized** Remote IDs (separators stripped) — callers normalize
//! via `wado_protocol::relay::normalize_remote_id` before touching the registry.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;

use dashmap::DashMap;
use tokio::sync::mpsc;

/// One registered wado-server.
#[allow(dead_code)]
pub struct RegisteredServer {
    /// Human-readable label shown in logs (optional, sent by the server).
    pub display_name: Option<String>,
    /// Remote address of the server's WS connection.
    pub addr: SocketAddr,
    /// Send a JSON string on this channel to deliver it to the server's WS.
    pub inbox_tx: mpsc::Sender<String>,
    pub registered_at: Instant,
}

#[derive(Default, Clone)]
pub struct ServerRegistry {
    inner: Arc<DashMap<String, RegisteredServer>>,
}

impl ServerRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a server under its (normalized) Remote ID. Returns `Err` if the
    /// ID is already taken by a live connection.
    pub fn insert(
        &self,
        remote_id: String,
        display_name: Option<String>,
        addr: SocketAddr,
        inbox_tx: mpsc::Sender<String>,
    ) -> Result<(), String> {
        if self.inner.contains_key(&remote_id) {
            return Err(format!("Remote ID '{remote_id}' is already registered"));
        }
        self.inner.insert(
            remote_id,
            RegisteredServer { display_name, addr, inbox_tx, registered_at: Instant::now() },
        );
        Ok(())
    }

    /// Remove a server (called on WS disconnect).
    pub fn remove(&self, remote_id: &str) {
        self.inner.remove(remote_id);
    }

    /// Look up a server by (normalized) Remote ID. Knowing a valid Remote ID is
    /// what authorizes a join — there is no separate password (the future
    /// confirmation gate asks the server itself to approve each peer).
    pub fn lookup(&self, remote_id: &str) -> Option<(mpsc::Sender<String>, Option<String>)> {
        let entry = self.inner.get(remote_id)?;
        Some((entry.inbox_tx.clone(), entry.display_name.clone()))
    }

    pub fn count(&self) -> usize {
        self.inner.len()
    }
}
