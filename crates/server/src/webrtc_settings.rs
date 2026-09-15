//! Shared WebRTC `SettingEngine` configuration — the single place that owns ICE
//! tuning for BOTH the direct (`website`) and relay (`relay_client`) transports,
//! so the two paths can never drift apart.
//!
//! Three knobs live here:
//! - **Forgiving ICE timeouts** — a brief connectivity gap must not drop a
//!   healthy localhost/LAN stream (the default 5 s → disconnected is too
//!   aggressive for our use).
//! - **Pinned ephemeral UDP port range** — WebRTC media/ICE otherwise binds
//!   *random* high UDP ports, which a host firewall cannot sanely open. Pinning
//!   the range to [`WEBRTC_UDP_PORT_MIN`]..=[`WEBRTC_UDP_PORT_MAX`] lets the
//!   firewall open exactly those ports: `udp 50000:50400`.
//!
//!   **Every daemon in a pool must get its own slice** — see [`udp_port_range`].
//! - **IPv6 gathering, switched off when the host has no IPv6 route** — see
//!   [`has_global_ipv6`]. This is the difference between a connection and a hang.

use std::time::Duration;

use webrtc::api::setting_engine::SettingEngine;
use webrtc::ice::network_type::NetworkType;
use webrtc::ice::udp_network::{EphemeralUDP, UDPNetwork};

/// Lowest UDP port WebRTC will bind for ICE/media.
///
/// Open this range in the host firewall, e.g. on NixOS:
/// `networking.firewall.allowedUDPPortRanges = [ { from = 50000; to = 50400; } ];`
pub const WEBRTC_UDP_PORT_MIN: u16 = 50000;

/// Highest UDP port WebRTC will bind for ICE/media. See [`WEBRTC_UDP_PORT_MIN`].
///
/// Widened from 50100 on `2026-09-14` so a pool of daemons can be given a slice each.
pub const WEBRTC_UDP_PORT_MAX: u16 = 50400;

/// How many ports one daemon gets. 100 is what a single daemon had before pools existed, and
/// it was enough; I14 (ports leaked per negotiation) is a separate bug and widening the slice
/// would only hide it for longer.
const PORTS_PER_INSTANCE: u16 = 100;

/// The UDP port slice this process should bind, from `WADO_UDP_SLICE`.
///
/// **Daemons in a pool must not share a port range.** Every `wado` process pinned
/// 50000–50100, so with four daemons the ICE sockets of four independent sessions collided in
/// one 101-port window: candidates were advertised on ports another process held, and every
/// session went `Checking → Failed` while each daemon's own logs looked perfectly healthy.
/// Measured on `2026-09-14`, immediately after the pool landed — all four daemons failing,
/// where a single daemon had been connecting in under a second an hour earlier.
///
/// `WADO_UDP_SLICE=n` (0-based) shifts this process to `MIN + n*100 ..= MIN + n*100 + 99`.
/// Unset means slice 0, so a lone daemon keeps exactly its old range and its old firewall rule.
/// A slice that would run past [`WEBRTC_UDP_PORT_MAX`] is refused loudly rather than silently
/// wrapping onto a sibling's ports — that is the failure this whole function exists to prevent.
fn udp_port_range() -> (u16, u16) {
    let slice: u16 = std::env::var("WADO_UDP_SLICE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let min = WEBRTC_UDP_PORT_MIN.saturating_add(slice.saturating_mul(PORTS_PER_INSTANCE));
    let max = min.saturating_add(PORTS_PER_INSTANCE - 1);
    if max > WEBRTC_UDP_PORT_MAX {
        tracing::error!(
            slice,
            "WADO_UDP_SLICE={slice} would need ports {min}-{max}, past the pinned ceiling \
             {WEBRTC_UDP_PORT_MAX}. Falling back to slice 0 — if another daemon is using it, \
             both will fail ICE. Raise WEBRTC_UDP_PORT_MAX and the firewall rule instead."
        );
        return (WEBRTC_UDP_PORT_MIN, WEBRTC_UDP_PORT_MIN + PORTS_PER_INSTANCE - 1);
    }
    (min, max)
}

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
    let (udp_min, udp_max) = udp_port_range();
    // Logged unconditionally, because two daemons silently sharing a range is invisible in
    // either one's log and shows up only as ICE that never completes.
    tracing::info!(udp_min, udp_max, "WebRTC ICE/media UDP ports {udp_min}-{udp_max}");
    match EphemeralUDP::new(udp_min, udp_max) {
        Ok(udp) => engine.set_udp_network(UDPNetwork::Ephemeral(udp)),
        Err(e) => {
            tracing::warn!("WebRTC UDP port range pin failed ({e}); using ephemeral default");
        }
    }

    // Do not gather over IPv6 on a host that has none.
    //
    // This cost a debugging session and looked exactly like a broken phone. With only a
    // link-local `fe80::` address and no IPv6 route, every v6 attempt fails — binding the
    // link-local ("could not listen udp fe80::…: invalid port number"), resolving a STUN name
    // that has only AAAA reachability ("No available ipv6 IP"), and the probe itself
    // ("Network is unreachable"). None of that is free: the failures run before the working
    // IPv4 probe and eat the gather deadline, so the answer goes out with **host candidates
    // only** and a client on another network can never reach it. ICE then goes
    // checking → disconnected → failed while the client sits on "starting session".
    //
    // Measured on the host where this was found: raw IPv4 STUN answered in 4–24 ms from three
    // different servers, while webrtc-rs reported "deadline has elapsed" on the same servers.
    // The network was never the problem.
    //
    // Detected rather than hardcoded, so an actually-dual-stack host still gathers v6.
    if !has_global_ipv6() {
        tracing::info!("no global IPv6 on this host — ICE gathers over IPv4 only");
        engine.set_network_types(vec![NetworkType::Udp4]);
    }

    engine
}

/// Whether this host has a globally-scoped IPv6 address.
///
/// Read from `/proc/net/if_inet6` rather than pulling in an interface-enumeration crate: the
/// fourth column is the address scope, and `0` is global. Loopback (`::1`, scope `0x10`) and
/// link-local (`fe80::`, scope `0x20`) are therefore excluded for free.
///
/// A read failure answers "no IPv6", which is the safe direction: the worst case is that a
/// dual-stack host gathers IPv4 only, which still connects.
fn has_global_ipv6() -> bool {
    let Ok(text) = std::fs::read_to_string("/proc/net/if_inet6") else {
        return false;
    };
    text.lines()
        .filter_map(|l| l.split_whitespace().nth(3))
        .any(|scope| u32::from_str_radix(scope, 16) == Ok(0))
}

#[cfg(test)]
mod tests {
    /// The parse, not the machine: this asserts how `/proc/net/if_inet6` is read, so the test
    /// gives the same answer on a v6 host and a v4-only one.
    #[test]
    fn scope_column_picks_global_only() {
        fn global(text: &str) -> bool {
            text.lines()
                .filter_map(|l| l.split_whitespace().nth(3))
                .any(|s| u32::from_str_radix(s, 16) == Ok(0))
        }
        // Loopback (scope 10) + link-local (scope 20) — what a v4-only host looks like.
        let v4_only = "00000000000000000000000000000001 01 80 10 80 lo\n                       fe8000000000000067b459d52a91008c 03 40 20 80 wlp97s0";
        assert!(!global(v4_only));
        // Same, plus a global address (scope 00).
        let dual = format!("{v4_only}\n2401db000000000000000000dead0001 03 40 00 80 wlp97s0");
        assert!(global(&dual));
        assert!(!global(""));
    }
}
