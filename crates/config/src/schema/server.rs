//! `server { }` — how the daemon is reached. Read once at startup: a listen address or a relay
//! identity cannot change under a connected viewer.

use serde::{Deserialize, Serialize};

/// The public relay every daemon dials unless told otherwise (Decision Log `2026-10-27`):
/// signalling only, so one small VPS serves everyone. `server { relay }` / `WADO_RELAY_URL`
/// point a daemon at your own; `relay ""` or `wado daemon <addr>` means direct mode.
/// The client's `DEFAULT_RELAY` names the same host.
pub const DEFAULT_RELAY: &str = "wss://wado-relay.indiasouthcentral.cloudapp.azure.com";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Server {
    /// Direct-mode HTTP control address. Localhost on purpose — see the RCE hazard in CLAUDE.md.
    pub listen: String,
    /// Relay URL — [`DEFAULT_RELAY`] unless set. Empty means direct mode.
    pub relay: Option<String>,
    /// Stable identity within a pool (`WADO_INSTANCE`).
    pub instance: String,
    /// Pin the Remote ID instead of using the generated, persisted one.
    pub remote_id: Option<String>,
    /// Which 100-port UDP slice this daemon uses, so pooled daemons never share ports.
    pub udp_slice: u16,
    pub turn: Option<Turn>,
    /// The relay as a phone reaches it — the public URL, not the local one the daemon dials —
    /// for the connect link and QR code (`wado qr`). `WADO_PUBLIC_RELAY` wins.
    pub public_relay: Option<String>,
    /// Where the web client is served; the connect link opens it.
    pub client_url: String,
    /// Sessions this computer runs at once. Each is its own daemon; "Start new" on a busy one
    /// starts a sibling daemon up to this many (Decision Log `2026-10-30`).
    pub max_sessions: u32,
    /// Devices in one session. The link speed test may lower it, never raise it.
    pub max_participants: u32,
}

impl Default for Server {
    fn default() -> Self {
        Self {
            listen: "127.0.0.1:8080".into(),
            relay: Some(DEFAULT_RELAY.into()),
            instance: "1".into(),
            remote_id: None,
            udp_slice: 0,
            turn: None,
            public_relay: None,
            client_url: "https://sandptel.github.io/wado/".into(),
            max_sessions: 4,
            max_participants: 4,
        }
    }
}

impl Server {
    /// The relay to dial, or `None` for direct mode.
    pub fn relay_url(&self) -> Option<&str> {
        self.relay.as_deref().filter(|r| !r.trim().is_empty())
    }

    /// The relay as a phone reaches it: `public-relay`, else the dialled relay when it is not
    /// on this machine (a loopback relay is no use to a phone), as `https://`.
    pub fn public_url(&self) -> Option<String> {
        if let Some(p) = &self.public_relay {
            return Some(p.clone());
        }
        let r = self.relay_url()?;
        let host = r.split("://").nth(1).unwrap_or(r);
        if ["127.", "localhost", "[::1]", "0.0.0.0"]
            .iter()
            .any(|l| host.starts_with(l))
        {
            return None;
        }
        Some(
            r.replacen("wss://", "https://", 1)
                .replacen("ws://", "http://", 1),
        )
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_url_follows_the_relay_unless_local() {
        let mut s = Server::default();
        assert_eq!(s.relay_url(), Some(DEFAULT_RELAY));
        assert_eq!(
            s.public_url().unwrap(),
            DEFAULT_RELAY.replacen("wss", "https", 1)
        );
        s.relay = Some("ws://127.0.0.1:4000".into());
        assert_eq!(s.public_url(), None);
        s.public_relay = Some("https://p.example".into());
        assert_eq!(s.public_url().as_deref(), Some("https://p.example"));
        s.relay = Some(" ".into());
        assert_eq!(s.relay_url(), None, "empty is direct mode");
    }
}
