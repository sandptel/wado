//! Can a peer actually use the server-reflexive candidate this host advertises?
//!
//! ICE assumes a srflx candidate is reusable: the address a STUN server saw is the address the
//! *peer* should send to. Behind a **symmetric** NAT that assumption is false — the mapping is
//! per destination, so the peer is told a port that will never accept its packets. Every
//! connection then fails identically and silently: `checking`, nothing, timeout. Nothing in the
//! SDP distinguishes it from a healthy srflx, which is exactly why it is worth a startup probe.
//!
//! Measured here on 2026-09-19. A VPN was the cause — Cloudflare WARP on the default route,
//! handing out a different external port per destination from one local socket:
//!
//! ```text
//! stun.cloudflare.com     -> 104.28.155.88:10189
//! stun.l.google.com       -> 104.28.155.88:10257
//! global.stun.twilio.com  -> 104.28.155.88:11497
//! ```
//!
//! That state had been diagnosed three times before as CGNAT and as access-point isolation,
//! neither of which was ever measured on this host. One line at startup replaces the guessing.
//!
//! ponytail: two probes, not the full RFC 5780 behaviour discovery. Two differing mappings is
//! already proof; a third adds nothing a human would act on differently.

use std::net::SocketAddr;
use std::time::Duration;

use tokio::net::UdpSocket;
use tracing::{debug, info, warn};
use webrtc::stun::agent::TransactionId;
use webrtc::stun::message::{Getter as _, Message, BINDING_REQUEST};
use webrtc::stun::xoraddr::XorMappedAddress;

/// How long one STUN round trip may take. A server that is going to answer answers in
/// 70–100 ms from this host; the cap is generous and bounds the whole probe at ~2 × this.
const RTT_BUDGET: Duration = Duration::from_secs(3);

/// What two probes from one socket say about this host's NAT.
#[derive(Debug, PartialEq)]
pub enum Verdict {
    /// Both servers saw the same address. A peer can use our srflx candidate.
    Consistent(SocketAddr),
    /// Different mapping per destination: our srflx candidate is not reusable by anyone.
    Symmetric(SocketAddr, SocketAddr),
    /// Fewer than two servers answered — says nothing either way.
    Unknown,
}

/// Classify what the probes saw. Split out from the I/O so it can be tested.
fn classify(seen: &[SocketAddr]) -> Verdict {
    match seen {
        [a, b, ..] if a == b => Verdict::Consistent(*a),
        [a, b, ..] => Verdict::Symmetric(*a, *b),
        _ => Verdict::Unknown,
    }
}

/// Ask one STUN server what it sees, over an existing socket.
///
/// The socket is shared across servers on purpose — that is the entire measurement. Two sockets
/// would get two mappings from any NAT and prove nothing.
async fn mapped_via(sock: &UdpSocket, server: &str) -> Option<SocketAddr> {
    let dst = tokio::net::lookup_host(server)
        .await
        .ok()?
        .find(|a| a.is_ipv4())?;

    let mut req = Message::new();
    req.build(&[Box::new(TransactionId::new()), Box::new(BINDING_REQUEST)])
        .ok()?;
    sock.send_to(&req.raw, dst).await.ok()?;

    let mut buf = [0u8; 1500];
    let n = tokio::time::timeout(RTT_BUDGET, sock.recv_from(&mut buf))
        .await
        .ok()?
        .ok()?
        .0;

    let mut resp = Message::new();
    resp.raw = buf[..n].to_vec();
    resp.decode().ok()?;
    let mut xor = XorMappedAddress::default();
    xor.get_from(&resp).ok()?;
    Some(SocketAddr::new(xor.ip, xor.port))
}

/// Probe, and say what it means. Spawned at startup; never blocks anything.
///
/// Deliberately a `warn!`, not a `debug!`: it is the difference between "connections fail for a
/// reason nothing in the log names" and one line saying which. It reaches the client's log panel
/// too, so the person holding the failing phone can see it.
pub async fn report() {
    let sock = match UdpSocket::bind("0.0.0.0:0").await {
        Ok(s) => s,
        Err(e) => return debug!("NAT probe: no socket ({e})"),
    };

    let mut seen = Vec::new();
    for server in crate::ice::stun_hosts() {
        if let Some(addr) = mapped_via(&sock, &server).await {
            debug!(%server, %addr, "NAT probe: mapping");
            seen.push(addr);
        }
        if seen.len() == 2 {
            break;
        }
    }

    match classify(&seen) {
        Verdict::Consistent(a) => {
            info!(public = %a, "NAT: one mapping for every destination — srflx candidates are usable")
        }
        Verdict::Symmetric(a, b) => warn!(
            "⛔ SYMMETRIC NAT — two STUN servers saw this host as {a} and {b} from the SAME local \
             socket. The srflx candidate in every answer is therefore a port no peer can reach, \
             and connections will hang in ICE `checking` with nothing else logged. A VPN on the \
             default route does this (Cloudflare WARP was the cause here on 2026-09-19); so does \
             a symmetric carrier NAT. Peers behind a cone NAT can still connect because we \
             initiate. Peers behind another symmetric NAT cannot connect at all without TURN."
        ),
        Verdict::Unknown => debug!("NAT probe: fewer than two STUN servers answered — no verdict"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_matching_mappings_are_a_cone_nat() {
        let a: SocketAddr = "1.2.3.4:5000".parse().unwrap();
        assert_eq!(classify(&[a, a]), Verdict::Consistent(a));
    }

    /// The case that matters: same IP, different port, is symmetric. Comparing only the IP —
    /// the obvious shortcut — would call this healthy, which is the exact failure it exists to
    /// catch.
    #[test]
    fn a_differing_port_alone_is_symmetric() {
        let a: SocketAddr = "1.2.3.4:5000".parse().unwrap();
        let b: SocketAddr = "1.2.3.4:6000".parse().unwrap();
        assert_eq!(classify(&[a, b]), Verdict::Symmetric(a, b));
    }

    #[test]
    fn one_answer_decides_nothing() {
        let a: SocketAddr = "1.2.3.4:5000".parse().unwrap();
        assert_eq!(classify(&[a]), Verdict::Unknown);
        assert_eq!(classify(&[]), Verdict::Unknown);
    }
}
