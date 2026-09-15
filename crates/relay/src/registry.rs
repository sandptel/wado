//! Server registry — tracks connected wado-server instances, keyed by Remote ID.
//!
//! **Several daemons may register under one Remote ID.** That is how wado serves more than
//! one device at a time: each daemon is a whole `wado` process with its own compositor,
//! encoder, Wayland socket and applications, and the relay hands each joining client a
//! different one. A session is a process, so the OS enforces the isolation and a segfault in
//! one device's graphics stack cannot reach another's — which is the same reason the
//! supervisor was a planned milestone (`WADO_PLAN.md`).
//!
//! Each registration gets an **instance id** minted here. It, not the Remote ID, is what a
//! room is keyed by — see [`crate::room`]. The Remote ID stays what a human types.
//!
//! On WS disconnect the entry is removed. All access is lock-free via DashMap. Keys are
//! **normalized** Remote IDs (separators stripped) — callers normalize via
//! `wado_protocol::relay::normalize_remote_id` before touching the registry.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;

use dashmap::DashMap;
use tokio::sync::mpsc;
use uuid::Uuid;

/// One registered wado-server.
#[allow(dead_code)]
pub struct RegisteredServer {
    /// This registration's identity, minted on insert. Stable for the life of the WS.
    pub instance_id: String,
    /// The Remote ID this daemon answers to. Shared with its pool siblings.
    pub remote_id: String,
    /// Human-readable label shown in logs (optional, sent by the server).
    pub display_name: Option<String>,
    /// Remote address of the server's WS connection.
    pub addr: SocketAddr,
    /// Send a JSON string on this channel to deliver it to the server's WS.
    pub inbox_tx: mpsc::Sender<String>,
    pub registered_at: Instant,
}

/// Registered daemons, keyed by **instance id** — not by Remote ID, because a Remote ID now
/// names a pool rather than a process.
#[derive(Default, Clone)]
pub struct ServerRegistry {
    inner: Arc<DashMap<String, RegisteredServer>>,
}

impl ServerRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a daemon under its (normalized) Remote ID, returning the minted instance id.
    ///
    /// Infallible, unlike the single-server version this replaces: a second daemon on the same
    /// Remote ID is a pool member, not a collision. Rejecting it was what made one Remote ID
    /// mean one device.
    pub fn insert(
        &self,
        remote_id: String,
        display_name: Option<String>,
        addr: SocketAddr,
        inbox_tx: mpsc::Sender<String>,
    ) -> String {
        let instance_id = Uuid::new_v4().to_string();
        self.inner.insert(
            instance_id.clone(),
            RegisteredServer {
                instance_id: instance_id.clone(),
                remote_id,
                display_name,
                addr,
                inbox_tx,
                registered_at: Instant::now(),
            },
        );
        instance_id
    }

    /// Remove one daemon (called on its WS disconnect).
    pub fn remove(&self, instance_id: &str) {
        self.inner.remove(instance_id);
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
            .map(|e| (e.registered_at, Instance {
                instance_id: e.instance_id.clone(),
                inbox_tx: e.inbox_tx.clone(),
                display_name: e.display_name.clone(),
            }))
            .collect();
        found.sort_by_key(|(t, _)| *t);
        found.into_iter().map(|(_, i)| i).collect()
    }

    /// One instance by its id, if it is still connected.
    pub fn get(&self, instance_id: &str) -> Option<Instance> {
        let e = self.inner.get(instance_id)?;
        Some(Instance {
            instance_id: e.instance_id.clone(),
            inbox_tx: e.inbox_tx.clone(),
            display_name: e.display_name.clone(),
        })
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
}
