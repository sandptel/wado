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

use webrtc::ice_transport::ice_server::RTCIceServer;

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
