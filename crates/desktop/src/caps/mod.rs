// SPDX-License-Identifier: AGPL-3.0-only
//! The capability seam: the **only** module that calls into the daemon's library (`wado`,
//! `wado_config`) or touches the host. Everything above it — `host/`, `client/`, the UI — sees
//! plain data and plain functions.
//!
//! It is shaped after the platform layer (`docs/superpowers/specs/2026-10-30-platform-layer-design.md`)
//! so it can switch to `wado-platform` without the UI noticing:
//!
//! - [`Cap`] / [`Absent`] mirror the registry's `Cap::On { imp, by } | Off(Absent { why, tried })`:
//!   a missing capability is honest data with a reason, never a hidden button.
//! - State is **pushed**: [`watch`] publishes snapshots on a `tokio::sync::watch` channel and the
//!   UI subscribes through `crate::bridge` (`upgrade_in_event_loop`). Today a poller produces
//!   them from the daemons' files and sockets; when the platform crate's stateful capabilities
//!   publish their own `watch`es, the poller's sources are swapped for those receivers.
//!
//! One file per job: [`daemon`] (the pool and the relay), [`devices`] (the trust list),
//! [`pairing`] (codes, QR, check words), [`sessions`], [`logs`], [`config`] (config.kdl),
//! [`computers`] (other computers, for client mode), [`desktop`] (this desktop: URLs, the
//! clipboard tool), [`video`] (native streaming — absent in this build).

pub mod computers;
pub mod config;
pub mod daemon;
pub mod desktop;
pub mod devices;
pub mod fixture;
pub mod logs;
pub mod pairing;
pub mod sessions;
pub mod video;
pub mod watch;

/// Who provides a capability — the platform layer's `ProviderId`, as a name.
pub type ProviderId = &'static str;

/// Why a capability is not here, and what was tried. Shown to the person, not logged away.
#[derive(Debug, Clone, PartialEq)]
pub struct Absent {
    pub why: String,
    pub tried: Vec<ProviderId>,
}

/// A capability: on, with who provides it, or off, with why.
#[derive(Debug, Clone, PartialEq)]
pub enum Cap<T> {
    On { imp: T, by: ProviderId },
    Off(Absent),
}

impl<T> Cap<T> {
    pub fn off(why: impl Into<String>, tried: &[ProviderId]) -> Self {
        Cap::Off(Absent {
            why: why.into(),
            tried: tried.to_vec(),
        })
    }
    pub fn is_on(&self) -> bool {
        matches!(self, Cap::On { .. })
    }
    pub fn why(&self) -> Option<&str> {
        match self {
            Cap::On { .. } => None,
            Cap::Off(a) => Some(&a.why),
        }
    }
}
