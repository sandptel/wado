//! One pairing ceremony. The daemon pairs only on the seat join (`pair=1`; never on a control
//! room, by design), so the ceremony takes the seat for as long as it lasts and leaves with
//! `4001` the moment it is let in (A9.4). No feature ever uses the seat.
//!
//! QR: the link's code is proven inside the envelope and its `hk` pins the computer, so a relay
//! cannot stand in for it. Typed code: proven with the session's own key, then four check words
//! shown here and on the computer, confirmed there within two minutes.
//!
//! Nothing is written until `e2e_ok`: the pin and the profile are saved together, after it. A
//! ceremony cut short (process death, a refusal) leaves nothing half-saved.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::time::Instant;
use wado_protocol::envelope::{sas_words, E2eMsg};
use wado_protocol::relay_wire::{display_remote_id, WireMsg};

use super::link::{PairCode, PairLink};
use crate::e2e::handshake::{qr_pin, ClientHandshake, HsError, Proof};
use crate::e2e::seal::Opener;
use crate::identity::Identity;
use crate::pins::{PinFrom, Pins};
use crate::profiles::{Profile, Profiles};
use crate::relay::frames::{classify, Frame};
use crate::relay::transport::{Connector, Transport, LEAVE};
use crate::relay::url;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PairStep {
    Dialing,
    Securing,
    /// A typed code: these four words are on the computer's screen too; confirm them there.
    CheckWords {
        words: [String; 4],
    },
    /// Proven; waiting for the computer to let this device in.
    WaitingForComputer,
    Done {
        key: String,
    },
    Failed {
        why: String,
    },
}

/// What a ceremony writes to, once it succeeds.
pub struct PairCtx {
    pub identity: Arc<Identity>,
    pub profiles: Arc<Mutex<Profiles>>,
    pub pins: Arc<Mutex<Pins>>,
    /// This device's name, as the computer will list it.
    pub name: String,
}

const DIAL: Duration = Duration::from_secs(10);
/// For the relay to put this device through to the computer.
const JOIN: Duration = Duration::from_secs(20);
/// For the computer to answer the handshake (as `E2E_REPLY_WAIT_MS`).
const REPLY: Duration = Duration::from_secs(8);
/// After a QR proof: the computer lets the device in at once.
const QR_LET_IN: Duration = Duration::from_secs(15);
/// After a typed proof: two minutes for a person to confirm the words, and a margin.
const TYPED_LET_IN: Duration = Duration::from_secs(130);

/// The relay's wording when the seat is someone else's (`crates/relay/src/signaling.rs`).
const OCCUPIED: &str = "already has an active connection";

fn failed(why: impl Into<String>) -> PairStep {
    PairStep::Failed { why: why.into() }
}

/// Run one ceremony to its end, reporting each step through `emit`. Returns the last step.
pub async fn run(
    link: PairLink,
    ctx: &PairCtx,
    connector: &dyn Connector,
    mut emit: impl FnMut(PairStep),
) -> PairStep {
    let last = ceremony(&link, ctx, connector, &mut emit).await;
    emit(last.clone());
    last
}

async fn ceremony(
    link: &PairLink,
    ctx: &PairCtx,
    connector: &dyn Connector,
    emit: &mut impl FnMut(PairStep),
) -> PairStep {
    let computer = display_remote_id(&link.remote_id);
    emit(PairStep::Dialing);
    let u = url::seat_pair(
        &link.relay_url,
        &link.remote_id,
        &ctx.identity.client_key,
        &ctx.name,
    );
    let mut t = match tokio::time::timeout(DIAL, connector.connect(&u)).await {
        Ok(Ok(t)) => t,
        _ => {
            return failed("Can't reach the computer's relay. Check the connection and try again.")
        }
    };
    let end = with_socket(link, ctx, &computer, t.as_mut(), emit).await;
    t.close(LEAVE).await;
    end
}

async fn next(t: &mut dyn Transport, until: Instant) -> Option<String> {
    match tokio::time::timeout_at(until, t.recv()).await {
        Ok(Some(Ok(text))) => Some(text),
        _ => None,
    }
}

async fn with_socket(
    link: &PairLink,
    ctx: &PairCtx,
    computer: &str,
    t: &mut dyn Transport,
    emit: &mut impl FnMut(PairStep),
) -> PairStep {
    let pin = match &link.hk {
        Some(hk) => Some(qr_pin(hk)),
        None => ctx.pins.lock().unwrap().get(&link.remote_id),
    };
    let proof = match &link.code {
        PairCode::Qr(c) => Proof::Qr(c.clone()),
        PairCode::Typed(c) => Proof::Typed(*c),
    };
    let mut hs: Option<ClientHandshake> = None;
    let mut opener: Option<Opener> = None;
    let mut host_pin = String::new();
    let mut until = Instant::now() + JOIN;
    loop {
        let Some(text) = next(t, until).await else {
            return failed(match (hs.is_some(), opener.is_some()) {
                (false, _) => format!("{computer} isn't online right now. Try again when it is."),
                (true, false) => {
                    format!("{computer} didn't answer the secure handshake — its wado may be older than the app.")
                }
                (true, true) => match link.code {
                    PairCode::Typed(_) => {
                        "Nobody confirmed the check words on the computer in time. Try again."
                            .into()
                    }
                    PairCode::Qr(_) => format!("{computer} didn't let this phone in. Try again."),
                },
            });
        };
        match classify(&text) {
            Frame::Wire(WireMsg::Ping) => {
                let _ = t.send(r#"{"type":"pong"}"#.into()).await;
            }
            Frame::Wire(WireMsg::Waiting { .. }) => {}
            Frame::Wire(WireMsg::JoinDenied { reason, .. }) => {
                return failed(if reason.contains(OCCUPIED) {
                    format!(
                        "{computer} is busy with another device's desktop. Try again in a moment, \
                         or pair from the computer."
                    )
                } else {
                    reason
                });
            }
            Frame::Wire(WireMsg::JoinAccepted { room_id, .. }) if hs.is_none() => {
                emit(PairStep::Securing);
                let (h, hello) = ClientHandshake::start(&room_id, &ctx.identity.client_key);
                hs = Some(h);
                if t.send(hello).await.is_err() {
                    return failed("The connection dropped. Try again.");
                }
                until = Instant::now() + REPLY;
            }
            Frame::E2e(E2eMsg::E2eReply { .. }) if opener.is_none() => {
                let Some(h) = hs.as_mut() else { continue };
                match h.on_reply(&text, &ctx.identity, pin.as_ref(), &proof) {
                    Ok(r) => {
                        host_pin = r.host_pin;
                        opener = Some(Opener::new(r.keys.d2c));
                        if t.send(r.finish).await.is_err() {
                            return failed("The connection dropped. Try again.");
                        }
                        until = Instant::now()
                            + match link.code {
                                PairCode::Qr(_) => QR_LET_IN,
                                PairCode::Typed(_) => TYPED_LET_IN,
                            };
                        if matches!(link.code, PairCode::Qr(_)) {
                            emit(PairStep::WaitingForComputer);
                        }
                    }
                    Err(HsError::PinMismatch | HsError::BadSignature) => {
                        return failed(
                            "This isn't the computer that showed the code — something between you \
                             may be standing in for it. Pair again from the computer's screen.",
                        );
                    }
                    Err(_) => return failed("The secure handshake failed. Try again."),
                }
            }
            Frame::E2e(E2eMsg::E2eSas { nonce }) => {
                let Some(h) = hs.as_ref() else { continue };
                match h.on_sas(&nonce) {
                    Ok(sas) => emit(PairStep::CheckWords {
                        words: sas_words(sas).map(str::to_string),
                    }),
                    Err(_) => {
                        return failed(
                            "The computer's check words aren't the ones it promised — something \
                             may be standing in for it. Pair again from the computer.",
                        )
                    }
                }
            }
            Frame::E2e(E2eMsg::E2eFail { reason }) => {
                return failed(if reason.is_empty() {
                    "The computer refused this phone.".to_string()
                } else {
                    reason
                });
            }
            Frame::Sealed => {
                let Some(op) = opener.as_mut() else { continue };
                let Ok(plain) = op.open(&text) else {
                    return failed("The secure link broke. Try again.");
                };
                if plain.starts_with(br#"{"type":"e2e_ok""#) {
                    return save(link, ctx, &host_pin);
                }
            }
            _ => {}
        }
    }
}

/// Let in: pin the computer and save its profile, together.
fn save(link: &PairLink, ctx: &PairCtx, host_pin: &str) -> PairStep {
    let from = if link.hk.is_some() {
        PinFrom::Qr
    } else {
        PinFrom::FirstUse
    };
    if ctx
        .pins
        .lock()
        .unwrap()
        .set(&link.remote_id, host_pin.to_string(), from)
        .is_err()
    {
        return failed("Paired, but this phone couldn't save it. Free some space and pair again.");
    }
    let mut profiles = ctx.profiles.lock().unwrap();
    let mut p = profiles
        .get(&wado_protocol::relay_wire::normalize_remote_id(
            &link.remote_id,
        ))
        .cloned()
        .unwrap_or_else(|| Profile::new(&link.relay_url, &link.remote_id));
    p.relay_url = link.relay_url.clone();
    let key = p.key.clone();
    profiles.upsert(p);
    if profiles.save().is_err() {
        return failed("Paired, but this phone couldn't save it. Free some space and pair again.");
    }
    PairStep::Done { key }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::relay::transport::MemTransport;
    use async_trait::async_trait;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
    use base64::Engine;
    use std::path::PathBuf;
    use wado::e2e::handshake::{finish, reply, Codes};
    use wado::e2e::host_key::HostKey;

    struct OneShot(Mutex<Option<MemTransport>>);

    #[async_trait]
    impl Connector for OneShot {
        async fn connect(&self, url: &str) -> std::io::Result<Box<dyn Transport>> {
            assert!(url.ends_with("&pair=1"), "{url}");
            Ok(Box::new(self.0.lock().unwrap().take().expect("one dial")))
        }
    }

    fn ctx(dir: &PathBuf) -> PairCtx {
        PairCtx {
            identity: Arc::new(Identity::ephemeral()),
            profiles: Arc::new(Mutex::new(Profiles::load(dir))),
            pins: Arc::new(Mutex::new(Pins::load(dir))),
            name: "Phone".into(),
        }
    }

    #[derive(Clone, Copy, PartialEq)]
    enum Says {
        Yes,
        No,
    }

    /// The relay and daemon on the far end of the socket, using the daemon's own handshake.
    async fn computer(
        mut t: MemTransport,
        h: HostKey,
        codes: Codes,
        says: Says,
        key: String,
    ) -> Option<u16> {
        t.send(
            r#"{"type":"join_accepted","remote_id":"123456789","room_id":"room","caps":[]}"#.into(),
        )
        .await
        .unwrap();
        let hello = t.recv().await?.ok()?;
        let E2eMsg::E2eHello { v, eph } = serde_json::from_str(&hello).ok()? else {
            return None;
        };
        // The relay tells the daemon the device's client key from the join URL.
        let (m, r) = reply(&h, "room", &key, v, &eph).ok()?;
        t.send(serde_json::to_string(&m).unwrap()).await.ok()?;
        let fin = t.recv().await?.ok()?;
        let E2eMsg::E2eFinish {
            dev_pk,
            sig,
            pair_mac,
            typed_mac,
        } = serde_json::from_str(&fin).ok()?
        else {
            return None;
        };
        let mut done = finish(r, &dev_pk, &sig, &pair_mac, &typed_mac, &codes).ok()?;
        if matches!(done.proof, wado::gate::Proof::Typed(_)) {
            t.send(
                serde_json::to_string(&E2eMsg::E2eSas {
                    nonce: done.sas_nonce.clone(),
                })
                .unwrap(),
            )
            .await
            .ok()?;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
        if says == Says::No {
            t.send(
                r#"{"type":"e2e_fail","reason":"The computer said the check words didn't match."}"#
                    .into(),
            )
            .await
            .ok()?;
        } else {
            let (n, c) = done.sealer.seal(br#"{"type":"e2e_ok"}"#);
            let ok = E2eMsg::Sealed {
                n,
                c: B64.encode(c),
            };
            t.send(serde_json::to_string(&ok).unwrap()).await.ok()?;
        }
        // Wait for the leave.
        while t.recv().await.is_some() {}
        *t.closed_by_peer.lock().unwrap()
    }

    async fn pair(
        code: PairCode,
        hk: bool,
        codes: Codes,
        says: Says,
    ) -> (Vec<PairStep>, Option<u16>, PathBuf, HostKey) {
        let dir = crate::store::temp_dir("pair");
        let c = ctx(&dir);
        let h = HostKey::load_or_create(&dir.join("host")).unwrap();
        let h2 = HostKey::load_or_create(&dir.join("host")).unwrap();
        let (a, b) = MemTransport::pair();
        let link = PairLink {
            relay_url: "https://relay.example".into(),
            remote_id: "123456789".into(),
            code,
            hk: hk.then(|| h.pin()),
        };
        let far = computer(b, h2, codes, says, c.identity.client_key.clone());
        let conn = OneShot(Mutex::new(Some(a)));
        let mut steps = Vec::new();
        let near = run(link, &c, &conn, |s| steps.push(s));
        let (_, closed) = tokio::join!(near, far);
        (steps, closed, dir, h)
    }

    #[tokio::test(flavor = "current_thread")]
    async fn qr_flow_saves_profile_and_pin_then_leaves() {
        let codes = Codes {
            qr: vec!["QRCODE".into()],
            typed: vec![],
        };
        let (steps, closed, dir, h) =
            pair(PairCode::Qr("QRCODE".into()), true, codes, Says::Yes).await;
        assert_eq!(
            steps.last(),
            Some(&PairStep::Done {
                key: "123456789".into()
            }),
            "{steps:?}"
        );
        assert!(steps.contains(&PairStep::WaitingForComputer));
        assert_eq!(closed, Some(4001), "left the seat");
        let pin = Pins::load(&dir).get("123456789").unwrap();
        assert_eq!(pin.pin, h.pin());
        assert_eq!(pin.from, PinFrom::Qr);
        assert_eq!(Profiles::load(&dir).list().len(), 1);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn typed_flow_shows_words_then_done_on_yes() {
        let codes = Codes {
            qr: vec![],
            typed: vec!["K7QF2MXP".into()],
        };
        let (steps, closed, dir, _) =
            pair(PairCode::Typed(*b"K7QF2MXP"), false, codes, Says::Yes).await;
        assert!(
            steps
                .iter()
                .any(|s| matches!(s, PairStep::CheckWords { words } if words.iter().all(|w| !w.is_empty()))),
            "{steps:?}"
        );
        assert!(matches!(steps.last(), Some(PairStep::Done { .. })));
        assert_eq!(closed, Some(4001));
        assert_eq!(
            Pins::load(&dir).get("123456789").unwrap().from,
            PinFrom::FirstUse
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn typed_flow_fails_plainly_on_no() {
        let codes = Codes {
            qr: vec![],
            typed: vec!["K7QF2MXP".into()],
        };
        let (steps, _, dir, _) = pair(PairCode::Typed(*b"K7QF2MXP"), false, codes, Says::No).await;
        assert_eq!(
            steps.last(),
            Some(&PairStep::Failed {
                why: "The computer said the check words didn't match.".into()
            })
        );
        assert!(Profiles::load(&dir).list().is_empty(), "nothing saved");
        assert!(Pins::load(&dir).get("123456789").is_none());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn profile_written_only_after_ok() {
        // The computer replies and takes the proof, then says nothing more: the ceremony runs
        // out, and nothing is on file.
        let dir = crate::store::temp_dir("pair-cut");
        let c = ctx(&dir);
        let (a, mut b) = MemTransport::pair();
        let conn = OneShot(Mutex::new(Some(a)));
        let link = PairLink {
            relay_url: "https://r".into(),
            remote_id: "5".into(),
            code: PairCode::Qr("Q".into()),
            hk: None,
        };
        let far = async {
            b.send(r#"{"type":"join_accepted","remote_id":"5","room_id":"r","caps":[]}"#.into())
                .await
                .unwrap();
            let _hello = b.recv().await;
            // No reply: drop the socket mid-ceremony.
            b.close(1006).await;
        };
        let near = run(link, &c, &conn, |_| {});
        let (last, _) = tokio::join!(near, far);
        assert!(matches!(last, PairStep::Failed { .. }), "{last:?}");
        assert!(crate::store::read(&dir, "profiles.json").is_none());
        assert!(crate::store::read(&dir, "pins.json").is_none());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn busy_seat_is_plain_failure() {
        let dir = crate::store::temp_dir("pair-busy");
        let c = ctx(&dir);
        let (a, mut b) = MemTransport::pair();
        let conn = OneShot(Mutex::new(Some(a)));
        let link = PairLink {
            relay_url: "https://r".into(),
            remote_id: "528491307".into(),
            code: PairCode::Qr("Q".into()),
            hk: None,
        };
        b.send(
            r#"{"type":"join_denied","reason":"528-491-307 already has an active connection (in use by Pixel)","takeover":true}"#
                .into(),
        )
        .await
        .unwrap();
        let last = run(link, &c, &conn, |_| {}).await;
        assert_eq!(
            last,
            PairStep::Failed {
                why: "528-491-307 is busy with another device's desktop. Try again in a moment, \
                      or pair from the computer."
                    .into()
            }
        );
    }
}
