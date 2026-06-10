//! Shared WebRTC `SettingEngine` configuration — the single place that owns ICE
//! tuning for BOTH the direct (`website`) and relay (`relay_client`) transports,
//! so the two paths can never drift apart.
//!
//! Two knobs live here:
//! - **Forgiving ICE timeouts** — a brief connectivity gap must not drop a
//!   healthy localhost/LAN stream (the default 5 s → disconnected is too
//!   aggressive for our use).
//! - **Pinned ephemeral UDP port range** — WebRTC media/ICE otherwise binds
//!   *random* high UDP ports, which a host firewall cannot sanely open. Pinning
//!   the range to [`WEBRTC_UDP_PORT_MIN`]..=[`WEBRTC_UDP_PORT_MAX`] lets the
//!   firewall open exactly those ports: `udp 50000:50100`.

use std::time::Duration;

use webrtc::api::setting_engine::SettingEngine;
use webrtc::ice::udp_network::{EphemeralUDP, UDPNetwork};

/// Lowest UDP port WebRTC will bind for ICE/media.
///
/// Open this range in the host firewall, e.g. on NixOS:
/// `networking.firewall.allowedUDPPortRanges = [ { from = 50000; to = 50100; } ];`
pub const WEBRTC_UDP_PORT_MIN: u16 = 50000;

/// Highest UDP port WebRTC will bind for ICE/media. See [`WEBRTC_UDP_PORT_MIN`].
pub const WEBRTC_UDP_PORT_MAX: u16 = 50100;

/// Build the `SettingEngine` shared by both WebRTC transports (direct + relay).
pub fn build_setting_engine() -> SettingEngine {
    let mut engine = SettingEngine::default();

    // Forgiving ICE timeouts (disconnected, failed, keepalive).
    engine.set_ice_timeouts(
        Some(Duration::from_secs(15)),
        Some(Duration::from_secs(30)),
        Some(Duration::from_secs(2)),
    );

    // Pin the ephemeral UDP port range so the host firewall can open exactly
    // these ports. The constants are a valid range (min <= max) so this never
    // errors; if it somehow did we log and fall back to the random default
    // rather than panic the server.
    match EphemeralUDP::new(WEBRTC_UDP_PORT_MIN, WEBRTC_UDP_PORT_MAX) {
        Ok(udp) => engine.set_udp_network(UDPNetwork::Ephemeral(udp)),
        Err(e) => {
            tracing::warn!("WebRTC UDP port range pin failed ({e}); using ephemeral default");
        }
    }

    engine
}
