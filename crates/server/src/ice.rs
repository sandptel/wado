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

/// The ICE configuration for every peer connection wado makes, in both relay and direct mode.
///
/// No TURN. A relayed candidate is the only thing that works behind symmetric NAT on both ends,
/// and it needs a server with a real public UDP address — the cloudflared quick tunnel in front
/// of the relay is HTTP only. That is roadmap iteration 3, not something this list can fake.
pub fn servers() -> Vec<RTCIceServer> {
    STUN.iter()
        .map(|u| RTCIceServer {
            urls: vec![(*u).to_owned()],
            ..Default::default()
        })
        .collect()
}

/// True when an SDP carries a server-reflexive candidate.
///
/// Worth checking explicitly rather than eyeballing the candidate count: host-only is not a
/// degraded connection, it is a connection that cannot happen from another network, and it is
/// the single most likely reason a phone hangs on "starting session…".
pub fn has_reflexive(sdp: &str) -> bool {
    sdp.lines().any(|l| l.contains("typ srflx") || l.contains("typ relay"))
}
