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
    let slice = wado_config::live::current().server.udp_slice;
    let min = WEBRTC_UDP_PORT_MIN.saturating_add(slice.saturating_mul(PORTS_PER_INSTANCE));
    let max = min.saturating_add(PORTS_PER_INSTANCE - 1);
    if max > WEBRTC_UDP_PORT_MAX {
        tracing::error!(
            slice,
            "WADO_UDP_SLICE={slice} would need ports {min}-{max}, past the pinned ceiling \
             {WEBRTC_UDP_PORT_MAX}. Falling back to slice 0 — if another daemon is using it, \
             both will fail ICE. Raise WEBRTC_UDP_PORT_MAX and the firewall rule instead."
        );
        return (
            WEBRTC_UDP_PORT_MIN,
            WEBRTC_UDP_PORT_MIN + PORTS_PER_INSTANCE - 1,
        );
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
    tracing::info!(
        udp_min,
        udp_max,
        "WebRTC ICE/media UDP ports {udp_min}-{udp_max}"
    );
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

/// Whether this host has a globally-routable IPv6 address on any interface.
///
/// Read with `getifaddrs`, which Linux and macOS both have (the old `/proc/net/if_inet6` read
/// answered "no" on every Mac, so a Mac never offered IPv6 and a phone behind carrier NAT could
/// only reach it through the relay). Loopback, link-local and unique-local (`fc00::/7`, which a
/// VPN's tunnel often carries) do not count.
///
/// A failure answers "no IPv6", which is the safe direction: the worst case is that a
/// dual-stack host gathers IPv4 only, which still connects.
fn has_global_ipv6() -> bool {
    let mut head: *mut libc::ifaddrs = std::ptr::null_mut();
    // SAFETY: getifaddrs fills `head` with a list we walk read-only and free once.
    if unsafe { libc::getifaddrs(&mut head) } != 0 {
        return false;
    }
    let mut found = false;
    let mut at = head;
    while !at.is_null() {
        // SAFETY: a node of the list getifaddrs returned, alive until freeifaddrs.
        let ifa = unsafe { &*at };
        if !ifa.ifa_addr.is_null()
            // SAFETY: a non-null sockaddr; its family says how long it is.
            && i32::from(unsafe { (*ifa.ifa_addr).sa_family }) == libc::AF_INET6
        {
            // SAFETY: an AF_INET6 sockaddr is a sockaddr_in6.
            let a = unsafe { &*(ifa.ifa_addr as *const libc::sockaddr_in6) };
            found |= is_global(std::net::Ipv6Addr::from(a.sin6_addr.s6_addr));
        }
        at = ifa.ifa_next;
    }
    // SAFETY: the list getifaddrs gave us, freed once.
    unsafe { libc::freeifaddrs(head) };
    found
}

fn is_global(a: std::net::Ipv6Addr) -> bool {
    !(a.is_loopback() || a.is_unspecified() || a.is_unicast_link_local() || a.is_unique_local())
}

#[cfg(test)]
mod tests {
    use super::is_global;

    #[test]
    fn only_routable_v6_counts() {
        for (a, g) in [
            ("::1", false),
            ("fe80::14a4:fdc0:23e3:954a", false),
            ("fc00::6440:1", false),
            ("2409:40c4:273:efe3:10ea:49ab:5ceb:f569", true),
            ("2401:db00::dead:1", true),
        ] {
            assert_eq!(is_global(a.parse().unwrap()), g, "{a}");
        }
    }

    /// This host, whatever it has: the call must not crash or leak.
    #[test]
    fn reads_this_host() {
        let _ = super::has_global_ipv6();
    }
}
