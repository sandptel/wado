//! How a connected device reaches this computer, as the device itself sees it, and what the
//! computer can say about its own network. Both ride the relay socket under the envelope
//! (Decision Log `2026-10-27`: the path must always be visible, on both ends).

use serde::{Deserialize, Serialize};

/// A device's link, reported every few seconds by the client ([`crate::relay::RelayMsg::DeviceStatus`]).
/// Every field defaults, so an older or newer client never breaks the daemon.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DeviceLink {
    /// `direct` | `relayed` | `reconnecting` | `offline` — the client's badge.
    pub state: String,
    /// With `direct`: `lan` | `internet` | `turn`, from the selected ICE candidate pair.
    pub path: String,
    /// Round trip on the media path, from that pair.
    pub rtt_ms: Option<u32>,
    /// The device's own network as the browser names it: `wifi`, `cellular`, … or empty.
    pub net: String,
    /// The page is in front of someone.
    pub visible: bool,
    /// The last reconnect took this long, and why it happened.
    pub last_recovery_ms: Option<u32>,
    pub last_recovery_why: String,
}

/// This computer's NAT, from the startup probe (`server::nat`), for the client's badge.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HostNet {
    /// `consistent` | `symmetric` | `unknown` | `` (not probed yet).
    pub nat: String,
    /// A TURN server is configured, so a symmetric NAT can still be crossed.
    pub turn: bool,
}
