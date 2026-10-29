//! The daemon's half of the handshake — see [`wado_protocol::envelope`] for the wire.

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use ring::agreement::{self, EphemeralPrivateKey, X25519};
use ring::digest::{SHA256, digest};
use ring::rand::{SecureRandom, SystemRandom};
use ring::{hmac, signature};
use wado_protocol::envelope::{E2eMsg, VERSION, transcript};

use super::host_key::HostKey;
use super::seal::{Opener, Sealer, keys, pair_key, sas, sas_key};
use crate::gate::Proof;

/// Replied, waiting for the client's `e2e_finish`.
pub struct Replied {
    t1: Vec<u8>,
    c2d: [u8; 32],
    d2c: [u8; 32],
    /// `k_pair`, for checking a typed code's proof.
    k_pair: [u8; 32],
    /// `k_sas` and the nonce `sas_commit` promised: the check words of a typed pairing.
    k_sas: [u8; 32],
    sas_nonce: [u8; 32],
}

/// A finished handshake: who the device is, and the link's keys.
pub struct Done {
    pub dev_pk: Vec<u8>,
    /// What the device proved about a pairing code. Nothing is spent or counted here.
    pub proof: Proof,
    /// The check words' bytes, and the nonce to reveal for the device to make the same ones.
    /// Used only for a typed code ([`Proof::Typed`]).
    pub sas: [u8; 4],
    pub sas_nonce: String,
    pub sealer: Sealer,
    pub opener: Opener,
}

/// Live pairing codes to check a proof against, in their two forms.
#[derive(Default)]
pub struct Codes {
    /// The QR's long codes.
    pub qr: Vec<String>,
    /// The typed codes, normalised.
    pub typed: Vec<String>,
}

fn b64(s: &str) -> Result<Vec<u8>, &'static str> {
    B64.decode(s).map_err(|_| "bad base64")
}

/// Answer an `e2e_hello` for `room_id`, whose device the relay says is `client_key`.
pub fn reply(
    host: &HostKey,
    room_id: &str,
    client_key: &str,
    v: u32,
    eph_c: &str,
) -> Result<(E2eMsg, Replied), &'static str> {
    if v != VERSION {
        return Err("unsupported envelope version");
    }
    let eph_c = b64(eph_c)?;
    let rng = SystemRandom::new();
    let mine = EphemeralPrivateKey::generate(&X25519, &rng).map_err(|_| "rng")?;
    let eph_d = mine
        .compute_public_key()
        .map_err(|_| "rng")?
        .as_ref()
        .to_vec();
    let t1 = transcript(room_id, client_key, &eph_c, &eph_d, host.public());
    let h1 = digest(&SHA256, &t1);
    let ((c2d, d2c), k_pair, k_sas) = agreement::agree_ephemeral(
        mine,
        &agreement::UnparsedPublicKey::new(&X25519, &eph_c),
        |shared| {
            (
                keys(shared, h1.as_ref()),
                pair_key(shared, h1.as_ref()),
                sas_key(shared, h1.as_ref()),
            )
        },
    )
    .map_err(|_| "bad ephemeral key")?;
    let mut sas_nonce = [0u8; 32];
    rng.fill(&mut sas_nonce).map_err(|_| "rng")?;
    let msg = E2eMsg::E2eReply {
        eph: B64.encode(&eph_d),
        host_pk: B64.encode(host.public()),
        sig: B64.encode(host.sign(h1.as_ref())),
        sas_commit: B64.encode(digest(&SHA256, &sas_nonce)),
    };
    Ok((
        msg,
        Replied {
            t1,
            c2d,
            d2c,
            k_pair,
            k_sas,
            sas_nonce,
        },
    ))
}

/// Check an `e2e_finish` against the live `codes`. A code a proof matches comes back in
/// [`Proof::Qr`] or [`Proof::Typed`] for the caller to redeem; a proof that matches none is
/// [`Proof::Stale`] (QR) or [`Proof::Wrong`] (typed). A QR proof is only checked against QR codes and a typed one only
/// against typed codes, so a guess at a short code is always counted as one.
pub fn finish(
    r: Replied,
    dev_pk: &str,
    sig: &str,
    pair_mac: &str,
    typed_mac: &str,
    codes: &Codes,
) -> Result<Done, &'static str> {
    let dev_pk = b64(dev_pk)?;
    let mut t2 = r.t1;
    t2.extend_from_slice(&dev_pk);
    let h2 = digest(&SHA256, &t2);
    signature::UnparsedPublicKey::new(&signature::ED25519, &dev_pk)
        .verify(h2.as_ref(), &b64(sig)?)
        .map_err(|_| "the device's signature does not verify")?;
    let qr = if pair_mac.is_empty() {
        None
    } else {
        let mac = b64(pair_mac)?;
        Some(
            codes
                .qr
                .iter()
                .find(|c| {
                    hmac::verify(
                        &hmac::Key::new(hmac::HMAC_SHA256, c.as_bytes()),
                        h2.as_ref(),
                        &mac,
                    )
                    .is_ok()
                })
                .cloned(),
        )
    };
    let typed = if typed_mac.is_empty() {
        None
    } else {
        let mac = b64(typed_mac)?;
        let key = hmac::Key::new(hmac::HMAC_SHA256, &r.k_pair);
        Some(
            codes
                .typed
                .iter()
                .find(|c| {
                    let msg = [h2.as_ref(), c.as_bytes()].concat();
                    hmac::verify(&key, &msg, &mac).is_ok()
                })
                .cloned(),
        )
    };
    let proof = match (qr, typed) {
        (Some(Some(c)), _) => Proof::Qr(c),
        (_, Some(Some(c))) => Proof::Typed(c),
        (_, Some(None)) => Proof::Wrong,
        (Some(None), None) => Proof::Stale,
        (None, None) => Proof::None,
    };
    Ok(Done {
        dev_pk,
        proof,
        sas: sas(&r.k_sas, h2.as_ref(), &r.sas_nonce),
        sas_nonce: B64.encode(r.sas_nonce),
        sealer: Sealer::new(&r.d2c),
        opener: Opener::new(&r.c2d),
    })
}

#[cfg(test)]
pub(crate) mod tests {
    //! A minimal client, enough to drive the daemon's half end to end.
    use super::*;
    use crate::e2e::seal::typed_mac;
    use ring::signature::{Ed25519KeyPair, KeyPair};

    pub struct Client {
        pub dev: Ed25519KeyPair,
        eph: Option<EphemeralPrivateKey>,
        pub eph_pub: Vec<u8>,
        /// After `answer`: `(k_sas, H(T2), sas_commit)`, for the check words.
        sas: Option<([u8; 32], Vec<u8>, String)>,
    }

    /// `(dev_pk, sig, pair_mac, typed_mac)`, as the browser sends them in `e2e_finish`.
    pub type Finish = (String, String, String, String);

    impl Client {
        pub fn new() -> Self {
            let rng = SystemRandom::new();
            let doc = Ed25519KeyPair::generate_pkcs8(&rng).unwrap();
            let eph = EphemeralPrivateKey::generate(&X25519, &rng).unwrap();
            let eph_pub = eph.compute_public_key().unwrap().as_ref().to_vec();
            Self {
                dev: Ed25519KeyPair::from_pkcs8(doc.as_ref()).unwrap(),
                eph: Some(eph),
                eph_pub,
                sas: None,
            }
        }
        /// The check words for the daemon's revealed nonce, as the browser makes them — `None`
        /// if the nonce is not the one the reply committed to.
        pub fn words(&self, nonce: &str) -> Option<[u8; 4]> {
            let (k, h2, commit) = self.sas.as_ref()?;
            let n = b64(nonce).ok()?;
            (B64.encode(digest(&SHA256, &n)) == *commit).then(|| sas(k, h2, &n))
        }
        /// `(e2e_finish fields, k_c2d, k_d2c)` for a reply, as the browser computes them — with
        /// a QR code `pair` and/or a typed code `typed` if it holds one.
        pub fn answer(
            &mut self,
            room: &str,
            key: &str,
            reply: &E2eMsg,
            pair: Option<&str>,
            typed: Option<&str>,
        ) -> (Finish, [u8; 32], [u8; 32]) {
            let E2eMsg::E2eReply {
                eph,
                host_pk,
                sas_commit,
                ..
            } = reply
            else {
                panic!()
            };
            let (eph_d, hpk) = (b64(eph).unwrap(), b64(host_pk).unwrap());
            let t1 = transcript(room, key, &self.eph_pub, &eph_d, &hpk);
            let h1 = digest(&SHA256, &t1);
            let ((c2d, d2c), k_pair, k_sas) = agreement::agree_ephemeral(
                self.eph.take().unwrap(),
                &agreement::UnparsedPublicKey::new(&X25519, &eph_d),
                |s| {
                    (
                        keys(s, h1.as_ref()),
                        pair_key(s, h1.as_ref()),
                        sas_key(s, h1.as_ref()),
                    )
                },
            )
            .unwrap();
            let mut t2 = t1;
            t2.extend_from_slice(self.dev.public_key().as_ref());
            let h2 = digest(&SHA256, &t2);
            self.sas = Some((k_sas, h2.as_ref().to_vec(), sas_commit.clone()));
            let mac = pair
                .map(|p| {
                    B64.encode(hmac::sign(
                        &hmac::Key::new(hmac::HMAC_SHA256, p.as_bytes()),
                        h2.as_ref(),
                    ))
                })
                .unwrap_or_default();
            let tmac = typed
                .map(|t| B64.encode(typed_mac(&k_pair, h2.as_ref(), t)))
                .unwrap_or_default();
            (
                (
                    B64.encode(self.dev.public_key()),
                    B64.encode(self.dev.sign(h2.as_ref())),
                    mac,
                    tmac,
                ),
                c2d,
                d2c,
            )
        }
    }

    fn host() -> HostKey {
        HostKey::from_seed(&[3u8; 32])
    }

    fn codes(qr: &[&str], typed: &[&str]) -> Codes {
        Codes {
            qr: qr.iter().map(|s| s.to_string()).collect(),
            typed: typed.iter().map(|s| s.to_string()).collect(),
        }
    }

    /// One handshake with a fresh client: what the daemon makes of its proof.
    fn proof(pair: Option<&str>, typed: Option<&str>, live: &Codes) -> Proof {
        let mut c = Client::new();
        let (m, r) = reply(&host(), "room", "ck", VERSION, &B64.encode(&c.eph_pub)).unwrap();
        let ((pk, sig, mac, tmac), ..) = c.answer("room", "ck", &m, pair, typed);
        finish(r, &pk, &sig, &mac, &tmac, live).unwrap().proof
    }

    #[test]
    fn a_full_handshake_agrees_on_keys_and_proves_the_pair_code() {
        let h = host();
        let mut c = Client::new();
        let (reply_msg, r) = reply(&h, "room", "ck", VERSION, &B64.encode(&c.eph_pub)).unwrap();
        // The client checks the host's signature over T1.
        let E2eMsg::E2eReply { eph, sig, .. } = &reply_msg else {
            panic!()
        };
        let t1 = transcript("room", "ck", &c.eph_pub, &b64(eph).unwrap(), h.public());
        signature::UnparsedPublicKey::new(&signature::ED25519, h.public())
            .verify(digest(&SHA256, &t1).as_ref(), &b64(sig).unwrap())
            .expect("host signature verifies");
        let ((pk, sig, mac, tmac), c2d, d2c) =
            c.answer("room", "ck", &reply_msg, Some("CODE"), None);
        let live = codes(&["OTHER", "CODE"], &[]);
        let mut done = finish(r, &pk, &sig, &mac, &tmac, &live).unwrap();
        assert_eq!(done.proof, Proof::Qr("CODE".into()));
        // Both directions open.
        let (n, ct) = Sealer::new(&c2d).seal(b"hi");
        assert_eq!(done.opener.open(n, ct).as_deref(), Some(&b"hi"[..]));
        let (n, ct) = done.sealer.seal(b"yo");
        assert_eq!(Opener::new(&d2c).open(n, ct).as_deref(), Some(&b"yo"[..]));
    }

    #[test]
    fn a_typed_code_is_proven_and_a_wrong_one_is_told_apart() {
        let live = codes(&["LONGQRCODE"], &["AAAA1111", "K7QF2MXP"]);
        assert_eq!(
            proof(None, Some("K7QF2MXP"), &live),
            Proof::Typed("K7QF2MXP".into())
        );
        assert_eq!(proof(None, Some("K7QF2MXQ"), &live), Proof::Wrong);
        assert_eq!(proof(None, None, &live), Proof::None);
        assert_eq!(proof(Some("USEDUP"), None, &live), Proof::Stale);
        // A typed code sent as a QR proof is checked against QR codes only — it cannot dodge
        // the count by coming in the other field.
        assert_eq!(proof(Some("K7QF2MXP"), None, &live), Proof::Stale);
        // …and a QR code sent as a typed proof is just a wrong guess.
        assert_eq!(proof(None, Some("LONGQRCODE"), &live), Proof::Wrong);
    }

    /// One typed-code handshake: the daemon's check words and the client's, from the nonce the
    /// daemon reveals.
    fn both_ends_words() -> ([u8; 4], Option<[u8; 4]>, Done) {
        let mut c = Client::new();
        let (m, r) = reply(&host(), "room", "ck", VERSION, &B64.encode(&c.eph_pub)).unwrap();
        let ((pk, sig, mac, tmac), ..) = c.answer("room", "ck", &m, None, Some("K7QF2MXP"));
        let done = finish(r, &pk, &sig, &mac, &tmac, &codes(&[], &["K7QF2MXP"])).unwrap();
        (done.sas, c.words(&done.sas_nonce), done)
    }

    #[test]
    fn the_check_words_match_on_both_ends_and_differ_across_sessions() {
        let (daemon, client, done) = both_ends_words();
        assert_eq!(done.proof, Proof::Typed("K7QF2MXP".into()));
        assert_eq!(Some(daemon), client, "same session, same words");
        let (other, ..) = both_ends_words();
        assert_ne!(daemon, other, "another session, other words");
        // The same 32-bit SAS twice in a row would be a 1-in-4-billion fluke; four in a row is a bug.
        let more: Vec<[u8; 4]> = (0..4).map(|_| both_ends_words().0).collect();
        assert!(more.iter().all(|w| *w != daemon));
    }

    #[test]
    fn the_client_refuses_a_nonce_that_was_not_promised() {
        let (_, _, done) = both_ends_words();
        let mut c = Client::new();
        let (m, _) = reply(&host(), "room", "ck", VERSION, &B64.encode(&c.eph_pub)).unwrap();
        c.answer("room", "ck", &m, None, Some("K7QF2MXP"));
        // A nonce from another handshake — a relay trying to steer this one's words — is not the
        // one this reply committed to.
        assert_eq!(c.words(&done.sas_nonce), None);
    }

    #[test]
    fn a_typed_proof_is_bound_to_the_session_not_just_the_transcript() {
        // The right code under a key anyone could compute does not verify: k_pair comes from the
        // X25519 secret, so the frame the relay sees is no oracle for the code.
        let live = codes(&[], &["K7QF2MXP"]);
        let mut c = Client::new();
        let (m, r) = reply(&host(), "room", "ck", VERSION, &B64.encode(&c.eph_pub)).unwrap();
        let ((pk, sig, ..), ..) = c.answer("room", "ck", &m, None, None);
        let public_only = B64.encode(typed_mac(&[0u8; 32], &[0u8; 32], "K7QF2MXP"));
        assert_eq!(
            finish(r, &pk, &sig, "", &public_only, &live).unwrap().proof,
            Proof::Wrong
        );
    }

    #[test]
    fn a_relay_that_rewrites_anything_is_caught() {
        let h = host();
        // The relay says the device is someone else: the transcripts differ, so the device's
        // signature (over what *it* saw) fails.
        let mut c = Client::new();
        let (m, r) = reply(
            &h,
            "room",
            "trusted-phone",
            VERSION,
            &B64.encode(&c.eph_pub),
        )
        .unwrap();
        let ((pk, sig, ..), ..) = c.answer("room", "attacker", &m, None, None);
        assert!(finish(r, &pk, &sig, "", "", &Codes::default()).is_err());
        // A wrong pair code proves nothing.
        assert_eq!(
            proof(Some("GUESS"), None, &codes(&["CODE"], &[])),
            Proof::Stale
        );
        // A future version is refused rather than misread.
        let c = Client::new();
        assert!(reply(&h, "room", "ck", VERSION + 1, &B64.encode(&c.eph_pub)).is_err());
    }
}
