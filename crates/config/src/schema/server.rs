//! `server { }` — how the daemon is reached. Read once at startup: a listen address or a relay
//! identity cannot change under a connected viewer.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Server {
    /// Direct-mode HTTP control address. Localhost on purpose — see the RCE hazard in CLAUDE.md.
    pub listen: String,
    /// Relay URL. Set means relay mode; unset means direct mode.
    pub relay: Option<String>,
    /// Stable identity within a pool (`WADO_INSTANCE`).
    pub instance: String,
    /// Pin the Remote ID instead of using the generated, persisted one.
    pub remote_id: Option<String>,
    /// Which 100-port UDP slice this daemon uses, so pooled daemons never share ports.
    pub udp_slice: u16,
    pub turn: Option<Turn>,
}

impl Default for Server {
    fn default() -> Self {
        Self {
            listen: "127.0.0.1:8080".into(),
            relay: None,
            instance: "1".into(),
            remote_id: None,
            udp_slice: 0,
            turn: None,
        }
    }
}

/// `turn { url "turn:host:3478"; user "u"; pass "p" }`
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Turn {
    #[serde(deserialize_with = "super::de::one_or_many")]
    pub url: Vec<String>,
    pub user: String,
    pub pass: String,
}
