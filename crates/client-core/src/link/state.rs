//! One computer's link, as a pure state machine: an input in (a transport event, a timer, a
//! command), actions out (dial, send, close, arm a timer, report). No I/O, no clock of its own:
//! [`super::driver`] runs it. The JS twin is `js/relay_link.js` + `js/e2e_handshake.js`, on a
//! seatless control room (`control=1&strict=1`) rather than the seat.
//!
//! Rules that are the point of this file:
//! - **No seat, ever.** A relay without `control` and `room_strict` would read the control room
//!   as a seat join: the link leaves (close `4001`) at once and sends nothing more.
//! - **Refused means stopped.** A pin mismatch, a bad signature or the computer's refusal ends the
//!   link with no redial: a human decides (no retry storm against an impostor).
//! - Nothing is sent to the computer until its signature and pin check out, and nothing it did
//!   not seal is acted on.

use std::sync::Arc;

use wado_protocol::envelope::E2eMsg;
use wado_protocol::relay::RelayMsg;
use wado_protocol::relay_wire::WireMsg;

use super::backoff;
use super::outbox::{Class, Next, Outbox};
use crate::e2e::handshake::{ClientHandshake, HsError, Proof};
use crate::e2e::seal::{Opener, Sealer};
use crate::identity::Identity;
use crate::pins::{Pin, PinFrom};
use crate::relay::frames::{classify, Frame};
use crate::relay::transport::LEAVE;
use crate::relay::url;

/// How long the computer has to answer `e2e_hello` (as `E2E_REPLY_WAIT_MS`).
pub const HELLO_WAIT_MS: u64 = 8000;
/// Three missed relay pings: the socket is dead even if the OS has not noticed.
pub const SILENCE_MS: u64 = 45_000;
/// How long a link must hold before its success resets the backoff.
pub const STABLE_MS: u64 = 5000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// The computer's key is not the pinned one (or its signature is bad).
    PinMismatch,
    /// The computer does not trust this device.
    NotTrusted,
    /// It did, and no longer does.
    Revoked,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkState {
    Idle,
    Dialing,
    Securing,
    Open {
        since_ms: u64,
    },
    /// Dropped after reaching the relay; dialing again in `in_ms`.
    Reconnecting {
        attempt: u32,
        in_ms: u64,
    },
    /// The relay itself cannot be reached; dialing again in `in_ms`.
    Unreachable {
        attempt: u32,
        in_ms: u64,
    },
    /// The relay is up and the computer is not there.
    Offline {
        seen_ms: Option<u64>,
    },
    Refused {
        why: Refusal,
    },
    RelayTooOld,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Timer {
    Retry,
    Hello,
    Silence,
    Pace,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Input {
    Start,
    Stop,
    /// The socket is open (the relay's verdict follows on it).
    Opened,
    /// The dial failed: the relay could not be reached.
    DialFailed,
    Frame(String),
    /// The socket closed or broke.
    Closed,
    Timer(Timer),
    /// A message for the computer, plaintext JSON.
    Send {
        text: String,
        class: Class,
        resend: bool,
    },
}

#[derive(Debug, Clone)]
pub enum Action {
    Dial(String),
    Send(String),
    /// Close the socket with this code. The driver does not report `Closed` for it.
    Close(u16),
    Arm(Timer, u64),
    Disarm(Timer),
    State(LinkState),
    /// An opened message from the computer.
    Msg(RelayMsg),
    /// Pin this computer on first use.
    SavePin(String),
    /// The relay's caps, from `join_accepted`.
    Caps(Vec<String>),
    /// The link is done for good: the driver returns.
    Halt,
}

/// `RelayMsg` has no `PartialEq`; actions compare by their debug form (tests only need this).
impl PartialEq for Action {
    fn eq(&self, other: &Self) -> bool {
        format!("{self:?}") == format!("{other:?}")
    }
}

/// Where and as whom.
#[derive(Debug, Clone)]
pub struct LinkCfg {
    pub key: String,
    pub relay_url: String,
    pub remote_id: String,
    /// This device's name, as the computer lists it.
    pub name: String,
}

pub struct Link {
    cfg: LinkCfg,
    identity: Arc<Identity>,
    pin: Option<Pin>,
    state: LinkState,
    connected: bool,
    /// Stopped or refused: never dial again.
    halted: bool,
    hs: Option<ClientHandshake>,
    opener: Option<Opener>,
    outbox: Outbox,
    attempt: u32,
    open_since: Option<u64>,
    last_open: Option<u64>,
    rng: u64,
}

const REQUIRED_CAPS: [&str; 2] = ["control", "room_strict"];

impl Link {
    pub fn new(cfg: LinkCfg, identity: Arc<Identity>, pin: Option<Pin>) -> Link {
        let mut seed = [0u8; 8];
        let _ = ring::rand::SecureRandom::fill(&ring::rand::SystemRandom::new(), &mut seed);
        Link {
            cfg,
            identity,
            pin,
            state: LinkState::Idle,
            connected: false,
            halted: false,
            hs: None,
            opener: None,
            outbox: Outbox::new(),
            attempt: 0,
            open_since: None,
            last_open: None,
            rng: u64::from_le_bytes(seed) | 1,
        }
    }

    pub fn state(&self) -> &LinkState {
        &self.state
    }

    pub fn cfg(&self) -> &LinkCfg {
        &self.cfg
    }

    fn jitter(&mut self) -> f64 {
        // xorshift64: enough to spread retries, no more.
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        (self.rng % 2001) as f64 / 1000.0 - 1.0
    }

    fn set(&mut self, s: LinkState, out: &mut Vec<Action>) {
        if self.state != s {
            self.state = s.clone();
            out.push(Action::State(s));
        }
    }

    fn url(&self) -> String {
        url::control(
            &self.cfg.relay_url,
            &self.cfg.remote_id,
            &self.identity.client_key,
            &self.cfg.name,
        )
    }

    /// The socket is gone (or being dropped): forget its keys, and plan the next dial.
    fn lost(&mut self, now_ms: u64, unreachable: bool, out: &mut Vec<Action>) {
        self.connected = false;
        self.hs = None;
        self.opener = None;
        self.outbox.link_lost();
        out.push(Action::Disarm(Timer::Hello));
        out.push(Action::Disarm(Timer::Silence));
        out.push(Action::Disarm(Timer::Pace));
        if let Some(since) = self.open_since.take() {
            self.last_open = Some(now_ms);
            if now_ms.saturating_sub(since) >= STABLE_MS {
                self.attempt = 0;
            }
        }
        if self.halted {
            return;
        }
        self.attempt += 1;
        let j = self.jitter();
        let in_ms = backoff::delay(self.attempt, j).as_millis() as u64;
        let s = if unreachable {
            LinkState::Unreachable {
                attempt: self.attempt,
                in_ms,
            }
        } else {
            LinkState::Reconnecting {
                attempt: self.attempt,
                in_ms,
            }
        };
        self.set(s, out);
        out.push(Action::Arm(Timer::Retry, in_ms));
    }

    /// End the link for good, leaving the relay (4001).
    fn halt(&mut self, s: LinkState, out: &mut Vec<Action>) {
        self.halted = true;
        if self.connected {
            out.push(Action::Close(LEAVE));
        }
        self.connected = false;
        self.hs = None;
        self.opener = None;
        self.outbox.link_lost();
        for t in [Timer::Retry, Timer::Hello, Timer::Silence, Timer::Pace] {
            out.push(Action::Disarm(t));
        }
        self.set(s, out);
    }

    fn drain(&mut self, now_ms: u64, out: &mut Vec<Action>) {
        if !matches!(self.state, LinkState::Open { .. }) {
            return;
        }
        loop {
            match self.outbox.next(now_ms as f64 / 1000.0) {
                Next::Frame(f) => out.push(Action::Send(f)),
                Next::Wait(s) => {
                    out.push(Action::Arm(Timer::Pace, (s * 1000.0).ceil() as u64));
                    break;
                }
                Next::Idle => break,
            }
        }
    }

    fn wire(&mut self, now_ms: u64, w: WireMsg, out: &mut Vec<Action>) {
        match w {
            WireMsg::Ping => out.push(Action::Send(r#"{"type":"pong"}"#.into())),
            WireMsg::JoinAccepted { room_id, caps, .. } => {
                out.push(Action::Caps(caps.clone()));
                if !REQUIRED_CAPS.iter().all(|c| caps.iter().any(|k| k == c)) {
                    // An older relay seated us. Leave at once; send nothing.
                    return self.halt(LinkState::RelayTooOld, out);
                }
                let (hs, hello) = ClientHandshake::start(&room_id, &self.identity.client_key);
                self.hs = Some(hs);
                self.set(LinkState::Securing, out);
                out.push(Action::Send(hello));
                out.push(Action::Arm(Timer::Hello, HELLO_WAIT_MS));
            }
            WireMsg::Waiting { .. } => {
                // Parked by the relay: no computer yet. The socket stays; join_accepted follows.
                self.set(
                    LinkState::Offline {
                        seen_ms: self.last_open,
                    },
                    out,
                );
            }
            WireMsg::JoinDenied { retry_ms, .. } => {
                // Not a fault of the network: say offline, and come back on the backoff (or the
                // relay's own "not before").
                self.connected = false;
                out.push(Action::Close(1000));
                self.attempt += 1;
                let j = self.jitter();
                let in_ms = (backoff::delay(self.attempt, j).as_millis() as u64).max(retry_ms);
                self.set(
                    LinkState::Offline {
                        seen_ms: self.last_open,
                    },
                    out,
                );
                out.push(Action::Arm(Timer::Retry, in_ms));
            }
            // Seats, takeovers and server-side frames are not a control room's.
            _ => {}
        }
        let _ = now_ms;
    }

    fn e2e(&mut self, now_ms: u64, text: &str, m: E2eMsg, out: &mut Vec<Action>) {
        match m {
            E2eMsg::E2eReply { .. } => {
                let Some(hs) = self.hs.as_mut() else { return };
                out.push(Action::Disarm(Timer::Hello));
                match hs.on_reply(text, &self.identity, self.pin.as_ref(), &Proof::None) {
                    Ok(r) => {
                        if r.first_use {
                            self.pin = Some(Pin {
                                pin: r.host_pin.clone(),
                                from: PinFrom::FirstUse,
                            });
                            out.push(Action::SavePin(r.host_pin));
                        }
                        self.opener = Some(Opener::new(r.keys.d2c));
                        self.outbox.rekey(Sealer::new(r.keys.c2d));
                        out.push(Action::Send(r.finish));
                        out.push(Action::Arm(Timer::Hello, HELLO_WAIT_MS));
                    }
                    Err(HsError::PinMismatch | HsError::BadSignature) => {
                        self.halt(
                            LinkState::Refused {
                                why: Refusal::PinMismatch,
                            },
                            out,
                        );
                    }
                    Err(_) => {
                        out.push(Action::Close(1000));
                        self.lost(now_ms, false, out);
                    }
                }
            }
            E2eMsg::E2eFail { .. } => {
                let why = if self.last_open.is_some() {
                    Refusal::Revoked
                } else {
                    Refusal::NotTrusted
                };
                self.halt(LinkState::Refused { why }, out);
            }
            // e2e_sas is a pairing's, never a control room's.
            _ => {}
        }
    }

    fn sealed(&mut self, now_ms: u64, text: &str, out: &mut Vec<Action>) {
        let Some(op) = self.opener.as_mut() else {
            return;
        };
        let plain = match op.open(text) {
            Ok(p) => p,
            Err(_) => {
                // Altered or lost: the link is no longer trusted. Drop it and dial afresh.
                out.push(Action::Close(1000));
                return self.lost(now_ms, false, out);
            }
        };
        let Ok(msg) = serde_json::from_slice::<serde_json::Value>(&plain) else {
            return;
        };
        if !matches!(self.state, LinkState::Open { .. }) {
            if msg.get("type").and_then(|t| t.as_str()) == Some("e2e_ok") {
                out.push(Action::Disarm(Timer::Hello));
                self.open_since = Some(now_ms);
                self.set(LinkState::Open { since_ms: now_ms }, out);
                self.drain(now_ms, out);
            }
            return;
        }
        if let Ok(m) = serde_json::from_value::<RelayMsg>(msg) {
            out.push(Action::Msg(m));
        }
    }
}

/// One step: `input` at `now_ms` → what to do.
pub fn step(l: &mut Link, now_ms: u64, input: Input) -> Vec<Action> {
    let mut out = Vec::new();
    match input {
        Input::Start => {
            if l.halted || l.connected {
                return out;
            }
            l.set(LinkState::Dialing, &mut out);
            out.push(Action::Disarm(Timer::Retry));
            out.push(Action::Dial(l.url()));
        }
        Input::Stop => {
            l.halt(LinkState::Idle, &mut out);
            out.push(Action::Halt);
        }
        Input::Opened => {
            if l.halted {
                out.push(Action::Close(LEAVE));
                return out;
            }
            l.connected = true;
            out.push(Action::Arm(Timer::Silence, SILENCE_MS));
        }
        Input::DialFailed => l.lost(now_ms, true, &mut out),
        Input::Closed => {
            if l.connected {
                l.lost(now_ms, false, &mut out);
            }
        }
        Input::Timer(Timer::Retry) => {
            if !l.halted && !l.connected {
                l.set(LinkState::Dialing, &mut out);
                out.push(Action::Dial(l.url()));
            }
        }
        Input::Timer(Timer::Hello) | Input::Timer(Timer::Silence) => {
            if l.connected {
                out.push(Action::Close(1000));
                l.lost(now_ms, false, &mut out);
            }
        }
        Input::Timer(Timer::Pace) => l.drain(now_ms, &mut out),
        Input::Send {
            text,
            class,
            resend,
        } => {
            if !l.halted {
                l.outbox.push(text, class, resend);
                l.drain(now_ms, &mut out);
            }
        }
        Input::Frame(text) => {
            if !l.connected {
                return out;
            }
            out.push(Action::Arm(Timer::Silence, SILENCE_MS));
            match classify(&text) {
                Frame::Wire(w) => l.wire(now_ms, w, &mut out),
                Frame::E2e(m) => l.e2e(now_ms, &text, m, &mut out),
                Frame::Sealed => l.sealed(now_ms, &text, &mut out),
                // In the clear and not the relay's: never acted on.
                Frame::Other => {}
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
    use base64::Engine;
    use wado::e2e::handshake::{finish, reply, Codes};
    use wado::e2e::host_key::HostKey;

    fn host() -> HostKey {
        HostKey::load_or_create(&crate::store::temp_dir("hk")).unwrap()
    }

    fn link(pin: Option<Pin>) -> Link {
        Link::new(
            LinkCfg {
                key: "1".into(),
                relay_url: "https://relay.example".into(),
                remote_id: "1".into(),
                name: "Phone".into(),
            },
            Arc::new(Identity::ephemeral()),
            pin,
        )
    }

    fn sent(a: &[Action]) -> Vec<String> {
        a.iter()
            .filter_map(|x| match x {
                Action::Send(s) => Some(s.clone()),
                _ => None,
            })
            .collect()
    }

    fn accepted(caps: &[&str]) -> Input {
        Input::Frame(
            serde_json::to_string(&WireMsg::JoinAccepted {
                remote_id: "1".into(),
                room_id: "room".into(),
                instance_id: String::new(),
                pool_size: 1,
                pool_busy: 0,
                assignment: String::new(),
                boot_id: String::new(),
                relay_v: 1,
                caps: caps.iter().map(|s| s.to_string()).collect(),
            })
            .unwrap(),
        )
    }

    fn connected(l: &mut Link) {
        let a = step(l, 0, Input::Start);
        assert!(matches!(&a[..], [.., Action::Dial(u)] if u.contains("control=1&strict=1")));
        step(l, 0, Input::Opened);
    }

    #[test]
    fn old_relay_never_takes_a_seat() {
        let mut l = link(None);
        connected(&mut l);
        let a = step(&mut l, 1, accepted(&["leave", "lane"]));
        assert!(a.contains(&Action::Close(4001)), "{a:?}");
        assert!(a.contains(&Action::State(LinkState::RelayTooOld)));
        assert!(sent(&a).is_empty(), "nothing sent: {a:?}");
        // Nothing after: no pong, no frames, no redial.
        assert!(step(&mut l, 2, Input::Frame(r#"{"type":"ping"}"#.into())).is_empty());
        let after = [
            step(&mut l, 3, Input::Closed),
            step(&mut l, 4, Input::Timer(Timer::Retry)),
            step(&mut l, 5, Input::Start),
        ]
        .concat();
        assert!(
            !after
                .iter()
                .any(|x| matches!(x, Action::Dial(_) | Action::Send(_))),
            "{after:?}"
        );
    }

    /// Drive a link through the daemon's own handshake; `pin` is what the link holds.
    fn to_reply(l: &mut Link, h: &HostKey) -> (Vec<Action>, wado::e2e::handshake::Replied) {
        connected(l);
        let a = step(l, 1, accepted(&["control", "room_strict"]));
        let hello = sent(&a).pop().expect("hello");
        let E2eMsg::E2eHello { v, eph } = serde_json::from_str(&hello).unwrap() else {
            panic!()
        };
        let (m, r) = reply(h, "room", &l.identity.client_key, v, &eph).unwrap();
        let a = step(l, 2, Input::Frame(serde_json::to_string(&m).unwrap()));
        (a, r)
    }

    #[test]
    fn refused_link_does_not_redial() {
        let (h, other) = (host(), host());
        let mut l = link(Some(Pin {
            pin: other.pin(),
            from: PinFrom::Qr,
        }));
        let (a, _) = to_reply(&mut l, &h);
        assert!(
            a.contains(&Action::State(LinkState::Refused {
                why: Refusal::PinMismatch
            })),
            "{a:?}"
        );
        assert!(a.contains(&Action::Close(4001)));
        assert!(sent(&a).is_empty(), "no finish to an impostor");
        let after = [
            step(&mut l, 3, Input::Closed),
            step(&mut l, 9000, Input::Timer(Timer::Retry)),
            step(&mut l, 9001, Input::Start),
        ]
        .concat();
        assert!(
            !after.iter().any(|x| matches!(x, Action::Dial(_))),
            "{after:?}"
        );
        // And the computer's own refusal is final too.
        let mut l = link(None);
        connected(&mut l);
        step(&mut l, 1, accepted(&["control", "room_strict"]));
        let a = step(
            &mut l,
            2,
            Input::Frame(r#"{"type":"e2e_fail","reason":"not paired"}"#.into()),
        );
        assert!(a.contains(&Action::State(LinkState::Refused {
            why: Refusal::NotTrusted
        })));
        assert!(step(&mut l, 3, Input::Timer(Timer::Retry)).is_empty());
    }

    #[test]
    fn join_denied_offline_is_offline_with_backoff() {
        let mut l = link(None);
        connected(&mut l);
        let a = step(
            &mut l,
            1,
            Input::Frame(
                r#"{"type":"join_denied","reason":"this computer's wado is older","retry_ms":3000}"#
                    .into(),
            ),
        );
        assert!(a.contains(&Action::State(LinkState::Offline { seen_ms: None })));
        let arm = a.iter().find_map(|x| match x {
            Action::Arm(Timer::Retry, ms) => Some(*ms),
            _ => None,
        });
        assert!(arm.is_some_and(|ms| ms >= 3000), "{a:?}");
        // The retry dials again.
        let a = step(&mut l, 4000, Input::Timer(Timer::Retry));
        assert!(a.iter().any(|x| matches!(x, Action::Dial(_))));
        // Parked by the relay is offline too, with the socket kept.
        step(&mut l, 4001, Input::Opened);
        let a = step(
            &mut l,
            4002,
            Input::Frame(r#"{"type":"waiting","reason":"no computer is online"}"#.into()),
        );
        assert!(!a.iter().any(|x| matches!(x, Action::Close(_))), "{a:?}");
    }

    #[test]
    fn open_link_seals_and_reports_messages_and_pins_first_use() {
        let h = host();
        let mut l = link(None);
        let (a, r) = to_reply(&mut l, &h);
        assert!(a.contains(&Action::SavePin(h.pin())), "{a:?}");
        let fin = sent(&a).pop().unwrap();
        let E2eMsg::E2eFinish {
            dev_pk,
            sig,
            pair_mac,
            typed_mac,
        } = serde_json::from_str(&fin).unwrap()
        else {
            panic!()
        };
        let mut done = finish(r, &dev_pk, &sig, &pair_mac, &typed_mac, &Codes::default()).unwrap();
        // A message queued before the link is up waits for it.
        let a = step(
            &mut l,
            3,
            Input::Send {
                text: r#"{"type":"host_get"}"#.into(),
                class: Class::Urgent,
                resend: true,
            },
        );
        assert!(sent(&a).is_empty());
        let seal = |done: &mut wado::e2e::handshake::Done, m: &str| {
            let (n, c) = done.sealer.seal(m.as_bytes());
            serde_json::to_string(&E2eMsg::Sealed {
                n,
                c: B64.encode(c),
            })
            .unwrap()
        };
        let ok = seal(&mut done, r#"{"type":"e2e_ok"}"#);
        let a = step(&mut l, 4, Input::Frame(ok));
        assert!(a.contains(&Action::State(LinkState::Open { since_ms: 4 })));
        let out = sent(&a);
        assert_eq!(out.len(), 1, "{a:?}");
        let E2eMsg::Sealed { n, c } = serde_json::from_str(&out[0]).unwrap() else {
            panic!()
        };
        assert_eq!(
            done.opener.open(n, B64.decode(c).unwrap()).as_deref(),
            Some(&br#"{"type":"host_get"}"#[..])
        );
        let st = seal(&mut done, r#"{"type":"sessions","sessions":[]}"#);
        let a = step(&mut l, 5, Input::Frame(st));
        assert!(
            matches!(&a[..], [.., Action::Msg(RelayMsg::Sessions { .. })]),
            "{a:?}"
        );
        // A dropped socket reconnects on the backoff, with no keys kept.
        let a = step(&mut l, 6, Input::Closed);
        assert!(a.iter().any(|x| matches!(x, Action::Arm(Timer::Retry, _))));
        assert!(a
            .iter()
            .any(|x| matches!(x, Action::State(LinkState::Reconnecting { .. }))));
        // A relay ping is answered in the clear.
        step(&mut l, 7, Input::Timer(Timer::Retry));
        step(&mut l, 7, Input::Opened);
        let a = step(&mut l, 8, Input::Frame(r#"{"type":"ping"}"#.into()));
        assert_eq!(sent(&a), [r#"{"type":"pong"}"#]);
    }

    #[test]
    fn stop_leaves_with_4001_and_halts() {
        let mut l = link(None);
        connected(&mut l);
        let a = step(&mut l, 1, Input::Stop);
        assert!(a.contains(&Action::Close(4001)));
        assert_eq!(a.last(), Some(&Action::Halt));
    }
}
