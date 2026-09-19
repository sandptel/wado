//! The STUN servers wado offers ICE, and why there is more than one.
//!
//! A server-reflexive candidate is what lets a phone on a different network reach this host.
//! Without one the answer carries only host candidates — a private LAN address and a link-local
//! IPv6 — and every connection from outside the LAN fails in the same silent way: ICE goes
//! `checking` → `disconnected` → `failed` while the client sits on "starting session…".
//!
//! **One STUN server was not enough on a 464XLAT/NAT64 link.** On 2026-09-12 this host gathered
//! srflx on roughly half its attempts: 26 `could not get server reflexive address … deadline has
//! elapsed` against `stun.l.google.com` in one afternoon, each producing a 2-candidate host-only
//! answer that could not connect. IPv4 egress worked and the name resolved — the UDP round trip
//! through the carrier's NAT64 simply did not come back inside the deadline often enough.
//!
//! The fix is redundancy, not a different single server. ICE gathers from every entry here in
//! parallel and needs only one to answer.

use std::time::Duration;

use webrtc::ice_transport::ice_server::RTCIceServer;

/// How long to wait for ICE gathering before sending the answer with whatever is in hand.
///
/// **This is the difference between connecting and hanging.** `webrtc-ice` gives every STUN
/// probe a hardcoded 5-second `STUN_GATHER_TIMEOUT` (`agent_gather.rs:21`) that no setting
/// exposes, and it does not wait on them in parallel. A probe to an unreachable server
/// therefore costs the full 5 s, and gathering-complete arrives 5, 10 or 15 seconds late
/// depending on how many died — measured exactly those three values on 2026-09-12, against
/// **30 milliseconds** for the one attempt that succeeded. The client gives up and re-offers
/// long before a 15-second answer arrives, so the connection can never be made and the log
/// shows nothing worse than a slow answer.
///
/// A STUN server that is going to reply replies in 4–24 ms from this host. Nothing is gained
/// by waiting seconds for one that has not. If the cap is hit the answer goes out with the
/// candidates gathered so far — which is strictly better than an answer nobody is still
/// waiting for.
pub const GATHER_WAIT: Duration = Duration::from_secs(2);

/// STUN servers, most-reliable first. All are public, free, and unauthenticated.
///
/// `stun1.l.google.com` is deliberately absent: it has no A record from this host, so it is a
/// guaranteed timeout that only slows gathering down.
const STUN: &[&str] = &[
    // Anycast, and the most consistent responder measured here.
    "stun:stun.cloudflare.com:3478",
    "stun:stun.l.google.com:19302",
    "stun:global.stun.twilio.com:3478",
];

/// The TURN server, when one is configured.
///
/// **This is the only candidate type that works when both ends are behind a symmetric NAT**, and
/// a VPN on either end is enough to cause that — measured 2026-09-19, WARP on the host and
/// Zscaler on the client, a connection that could not be made between two machines on one WiFi.
/// See [`crate::nat`] for how that state is detected and why no code change routes around it.
///
/// Configured from the environment rather than a config file because the daemon already takes
/// `WADO_RELAY_URL`, `WADO_REMOTE_ID` and `WADO_UDP_SLICE` that way, and a pool starts N of them
/// from one script. Absent means STUN only, which is what wado has always done.
///
/// ponytail: long-lived shared credentials, no REST/ephemeral-token scheme. The upgrade path is
/// coturn's `use-auth-secret` with time-limited usernames, and it matters only once the TURN
/// server is reachable by people who are not us.
fn turn() -> Option<RTCIceServer> {
    let urls: Vec<String> = std::env::var("WADO_TURN_URL")
        .ok()?
        .split(',')
        .map(str::trim)
        .filter(|u| !u.is_empty())
        .map(str::to_owned)
        .collect();
    if urls.is_empty() {
        return None;
    }
    // Said out loud, because a TURN server that is configured but unusable looks exactly like no
    // TURN at all: ICE simply never produces a `relay` candidate and the answer looks normal.
    if let Some(bad) = urls.iter().find(|u| !u.starts_with("turn:") && !u.starts_with("turns:")) {
        tracing::error!(
            "WADO_TURN_URL entry {bad:?} is not a turn: or turns: URL — webrtc-rs will reject it \
             and this daemon will fall back to STUN only, which cannot connect two peers that \
             are both behind a VPN or a symmetric NAT."
        );
        return None;
    }
    Some(RTCIceServer {
        urls,
        username: std::env::var("WADO_TURN_USER").unwrap_or_default(),
        credential: std::env::var("WADO_TURN_PASS").unwrap_or_default(),
        ..Default::default()
    })
}

/// Whether a TURN server is configured at all. Read by [`crate::nat`] so the symmetric-NAT
/// warning can say whether anything will rescue it.
pub fn has_turn() -> bool {
    turn().is_some()
}

/// The ICE configuration for every peer connection wado makes, in both relay and direct mode.
///
/// STUN always; TURN when `WADO_TURN_URL` is set. Without TURN a relayed candidate cannot exist,
/// and two peers behind symmetric NAT — which a VPN on either end produces — cannot connect at
/// all. See the `2026-09-19` Decision Log entry.
pub fn servers() -> Vec<RTCIceServer> {
    let mut out: Vec<RTCIceServer> = STUN
        .iter()
        .map(|u| RTCIceServer {
            urls: vec![(*u).to_owned()],
            ..Default::default()
        })
        .collect();
    if let Some(t) = turn() {
        tracing::info!(urls = ?t.urls, "TURN configured — relayed candidates are available");
        out.push(t);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One test, not three: these read a process-global env var and `cargo test` runs tests in
    /// parallel threads of one process, so split across `#[test]`s they race and fail at random.
    ///
    /// A scheme typo is the case worth covering — it is silent otherwise: ICE simply never
    /// gathers a relay candidate and the answer looks like a normal STUN-only one.
    #[test]
    fn turn_is_configured_from_the_environment_and_a_bad_scheme_is_refused() {
        unsafe { std::env::remove_var("WADO_TURN_URL") };
        assert!(turn().is_none(), "unset means STUN only");
        assert_eq!(servers().len(), STUN.len());

        unsafe { std::env::set_var("WADO_TURN_URL", "stun:example.org:3478") };
        assert!(turn().is_none(), "a stun: URL in the TURN slot is refused, not passed through");

        unsafe { std::env::set_var("WADO_TURN_URL", "turn:example.org:3478,turns:example.org:5349") };
        let t = turn().expect("both schemes accepted");
        assert_eq!(t.urls.len(), 2, "comma-separated entries are split");
        assert_eq!(servers().len(), STUN.len() + 1);

        unsafe { std::env::remove_var("WADO_TURN_URL") };
    }
}

/// The STUN servers as bare `host:port`, for anything that speaks STUN itself rather than
/// handing the list to webrtc-rs — see [`crate::nat`].
pub fn stun_hosts() -> Vec<String> {
    STUN.iter().map(|u| u.trim_start_matches("stun:").to_owned()).collect()
}

/// True when an SDP carries a server-reflexive candidate.
///
/// Worth checking explicitly rather than eyeballing the candidate count: host-only is not a
/// degraded connection, it is a connection that cannot happen from another network, and it is
/// the single most likely reason a phone hangs on "starting session…".
pub fn has_reflexive(sdp: &str) -> bool {
    sdp.lines().any(|l| l.contains("typ srflx") || l.contains("typ relay"))
}
