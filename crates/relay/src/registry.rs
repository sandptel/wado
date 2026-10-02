//! Server registry — tracks connected wado-server instances, keyed by Remote ID.
//!
//! **Several daemons may register under one Remote ID.** That is how wado serves more than
//! one device at a time: each daemon is a whole `wado` process with its own compositor,
//! encoder, Wayland socket and applications, and the relay hands each joining client a
//! different one. A session is a process, so the OS enforces the isolation and a segfault in
//! one device's graphics stack cannot reach another's — which is the same reason the
//! supervisor was a planned milestone (`WADO_PLAN.md`).
//!
//! Each registration gets an **instance id**. It, not the Remote ID, is what a room is keyed
//! by — see [`crate::room`]. The Remote ID stays what a human types.
//!
//! The instance id is `<remote_id>:<instance_key>` when the daemon sends a key
//! (`WADO_INSTANCE`), so it survives restarts of the daemon and of the relay, and a client
//! asking for it back by `?instance=` gets the same daemon. A daemon from before the key
//! existed gets a uuid per connection, as before.
//!
//! On WS disconnect the entry is removed. All access is lock-free via DashMap. Keys are
//! **normalized** Remote IDs (separators stripped) — callers normalize via
//! `wado_protocol::relay::normalize_remote_id` before touching the registry.
//!
//! Every insert and remove bumps a [`watch`](tokio::sync::watch) counter. A join parked
//! because its daemon is not here yet waits on that, rather than the client polling.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use dashmap::mapref::entry::Entry;
use dashmap::DashMap;
use tokio::sync::{mpsc, oneshot, watch};
use uuid::Uuid;

/// What a daemon told the relay about itself in `register`.
pub struct NewServer {
    pub remote_id: String,
    pub instance_key: String,
    pub boot_id: String,
    pub display_name: Option<String>,
    pub addr: SocketAddr,
    pub inbox_tx: mpsc::Sender<String>,
    /// How long to hold this daemon's seat for a viewer that drops (its `VIEWER_GRACE`).
    pub hold: Duration,
    /// The daemon decides each join (`peer_accept` / `peer_reject`).
    pub gate: bool,
}

/// One registered wado-server.
struct RegisteredServer {
    remote_id: String,
    registered_at: Instant,
    conn_id: String,
    /// Dropped with the entry. The connection's loop waits on the other end, so replacing the
    /// entry ends the stale connection at once instead of when its TCP finally dies.
    _kick: oneshot::Sender<()>,
    info: Instance,
}

/// What a successful registration hands back to its connection loop.
pub struct Registration {
    pub instance_id: String,
    pub conn_id: String,
    /// Resolves when this registration has been replaced — the loop should end.
    pub kicked: oneshot::Receiver<()>,
    /// This daemon redialled while the relay still held its old connection.
    pub replaced: bool,
}

/// Registered daemons, keyed by **instance id** — not by Remote ID, because a Remote ID now
/// names a pool rather than a process.
#[derive(Clone)]
pub struct ServerRegistry {
    inner: Arc<DashMap<String, RegisteredServer>>,
    changed: Arc<watch::Sender<u64>>,
}

impl Default for ServerRegistry {
    fn default() -> Self {
        Self {
            inner: Arc::default(),
            changed: Arc::new(watch::channel(0).0),
        }
    }
}

impl ServerRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// A receiver that wakes on every registration and departure.
    pub fn watch(&self) -> watch::Receiver<u64> {
        self.changed.subscribe()
    }

    fn bump(&self) {
        self.changed.send_modify(|n| *n += 1);
    }

    /// Register a daemon under its (normalized) Remote ID.
    ///
    /// A second daemon on the same Remote ID is a pool member, not a collision. The one
    /// refusal is a second **process** claiming a key already held: same key and same
    /// `boot_id` is one daemon that redialled before the relay noticed its old socket die, so
    /// it replaces that socket; a different `boot_id` is two daemons configured with one
    /// `WADO_INSTANCE`, and letting the newer win would have them evict each other on every
    /// reconnect (`issues.md` I17). That one is refused and the daemon logs why.
    pub fn insert(&self, s: NewServer) -> Result<Registration, String> {
        let instance_id = if s.instance_key.is_empty() {
            Uuid::new_v4().to_string()
        } else {
            format!("{}:{}", s.remote_id, s.instance_key)
        };
        let conn_id = Uuid::new_v4().to_string();
        let (kick, kicked) = oneshot::channel();
        let mut entry = RegisteredServer {
            remote_id: s.remote_id,
            registered_at: Instant::now(),
            conn_id: conn_id.clone(),
            _kick: kick,
            info: Instance {
                instance_id: instance_id.clone(),
                inbox_tx: s.inbox_tx,
                display_name: s.display_name,
                boot_id: s.boot_id,
                hold: s.hold,
                gate: s.gate,
            },
        };
        let replaced = match self.inner.entry(instance_id.clone()) {
            Entry::Vacant(slot) => {
                slot.insert(entry);
                false
            }
            Entry::Occupied(mut slot) => {
                if entry.info.boot_id.is_empty() || slot.get().info.boot_id != entry.info.boot_id {
                    return Err(format!(
                        "instance key {:?} is already registered by another running daemon — \
                         give each daemon its own WADO_INSTANCE",
                        s.instance_key
                    ));
                }
                // Keep the pool position: assignment is oldest-first, and a redial is not a
                // new daemon.
                entry.registered_at = slot.get().registered_at;
                slot.insert(entry); // drops the old entry → its `_kick` → old loop ends
                true
            }
        };
        self.bump();
        Ok(Registration {
            instance_id,
            conn_id,
            kicked,
            replaced,
        })
    }

    /// Remove a registration on its WS disconnect — **only if** it is still this connection's.
    /// Returns whether it was.
    pub fn remove_if(&self, instance_id: &str, conn_id: &str) -> bool {
        let gone = self
            .inner
            .remove_if(instance_id, |_, e| e.conn_id == conn_id)
            .is_some();
        if gone {
            self.bump();
        }
        gone
    }

    /// Every instance registered for a Remote ID, **oldest registration first**.
    ///
    /// Ordered so assignment is deterministic: the same device joining an idle pool twice
    /// lands on the same daemon, which is the cheapest form of stickiness available without
    /// the client saying anything. A `DashMap` iterates in no particular order, so the sort
    /// is what makes that true rather than a coincidence of hashing.
    pub fn instances_for(&self, remote_id: &str) -> Vec<Instance> {
        let mut found: Vec<_> = self
            .inner
            .iter()
            .filter(|e| e.remote_id == remote_id)
            .map(|e| (e.registered_at, e.info.clone()))
            .collect();
        found.sort_by_key(|(t, _)| *t);
        found.into_iter().map(|(_, i)| i).collect()
    }

    pub fn count(&self) -> usize {
        self.inner.len()
    }
}

/// What a caller needs to talk to one registered daemon. Cloned out of the map so no DashMap
/// reference is held across an await — holding one locks that shard behind a slow socket.
#[derive(Clone)]
pub struct Instance {
    pub instance_id: String,
    pub inbox_tx: mpsc::Sender<String>,
    pub display_name: Option<String>,
    pub boot_id: String,
    pub hold: Duration,
    pub gate: bool,
}

#[cfg(test)]
mod tests {
    use super::{NewServer, ServerRegistry};
    use std::time::Duration;
    use tokio::sync::mpsc;

    fn reg(r: &ServerRegistry, key: &str, boot: &str) -> Result<super::Registration, String> {
        let (tx, _rx) = mpsc::channel(1);
        r.insert(NewServer {
            remote_id: "528491307".into(),
            instance_key: key.into(),
            boot_id: boot.into(),
            display_name: None,
            addr: "127.0.0.1:1".parse().unwrap(),
            inbox_tx: tx,
            hold: Duration::ZERO,
            gate: false,
        })
    }

    #[test]
    fn keyed_daemons_get_stable_ids_and_redials_replace() {
        let r = ServerRegistry::new();
        let mut a = reg(&r, "1", "boot-a").unwrap();
        assert_eq!(a.instance_id, "528491307:1");
        assert!(!a.replaced);
        // Same process redialling: replaces, and the old connection is told to end.
        let b = reg(&r, "1", "boot-a").unwrap();
        assert!(b.replaced);
        assert!(a
            .kicked
            .try_recv()
            .is_err_and(|e| e == tokio::sync::oneshot::error::TryRecvError::Closed));
        // The stale connection's cleanup must not remove its replacement.
        assert!(!r.remove_if(&a.instance_id, &a.conn_id));
        assert_eq!(r.count(), 1);
        // A different process with the same key is refused, never allowed to steal.
        assert!(reg(&r, "1", "boot-other").is_err());
        assert!(r.remove_if(&b.instance_id, &b.conn_id));
        assert_eq!(r.count(), 0);
    }

    #[test]
    fn keyless_daemons_still_pool_as_before() {
        let r = ServerRegistry::new();
        let a = reg(&r, "", "").unwrap();
        let b = reg(&r, "", "").unwrap();
        assert_ne!(a.instance_id, b.instance_id);
        assert_eq!(r.instances_for("528491307").len(), 2);
    }

    #[test]
    fn parked_joins_are_woken_by_a_registration() {
        let r = ServerRegistry::new();
        let mut w = r.watch();
        w.borrow_and_update();
        let _a = reg(&r, "1", "b").unwrap();
        assert!(w.has_changed().unwrap());
    }
}
