//! The daemon's half of the handshake — see [`wado_protocol::envelope`] for the wire.

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use ring::agreement::{self, EphemeralPrivateKey, X25519};
use ring::digest::{SHA256, digest};
use ring::rand::SystemRandom;
use ring::{hmac, signature};
use wado_protocol::envelope::{E2eMsg, VERSION, transcript};

use super::host_key::HostKey;
use super::seal::{Opener, Sealer, keys};

/// Replied, waiting for the client's `e2e_finish`.
pub struct Replied {
    t1: Vec<u8>,
    c2d: [u8; 32],
    d2c: [u8; 32],
}

/// A finished handshake: who the device is, and the link's keys.
pub struct Done {
    pub dev_pk: Vec<u8>,
    /// The QR pairing code the device proved it holds, if it sent a proof that matched one.
    pub pair: Option<String>,
    pub sealer: Sealer,
    pub opener: Opener,
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
    let (c2d, d2c) = agreement::agree_ephemeral(
        mine,
        &agreement::UnparsedPublicKey::new(&X25519, &eph_c),
        |shared| keys(shared, h1.as_ref()),
    )
    .map_err(|_| "bad ephemeral key")?;
    let msg = E2eMsg::E2eReply {
        eph: B64.encode(&eph_d),
        host_pk: B64.encode(host.public()),
        sig: B64.encode(host.sign(h1.as_ref())),
    };
    Ok((msg, Replied { t1, c2d, d2c }))
}

/// Check an `e2e_finish`. `pair_codes` are the live QR codes; one that matches the proof is
/// returned for the caller to redeem.
pub fn finish(
    r: Replied,
    dev_pk: &str,
    sig: &str,
    pair_mac: &str,
    pair_codes: &[String],
) -> Result<Done, &'static str> {
    let dev_pk = b64(dev_pk)?;
    let mut t2 = r.t1;
    t2.extend_from_slice(&dev_pk);
    let h2 = digest(&SHA256, &t2);
    signature::UnparsedPublicKey::new(&signature::ED25519, &dev_pk)
        .verify(h2.as_ref(), &b64(sig)?)
        .map_err(|_| "the device's signature does not verify")?;
    let pair = if pair_mac.is_empty() {
        None
    } else {
        let mac = b64(pair_mac)?;
        pair_codes
            .iter()
            .find(|c| {
                hmac::verify(
                    &hmac::Key::new(hmac::HMAC_SHA256, c.as_bytes()),
                    h2.as_ref(),
                    &mac,
                )
                .is_ok()
            })
            .cloned()
    };
    Ok(Done {
        dev_pk,
        pair,
        sealer: Sealer::new(&r.d2c),
        opener: Opener::new(&r.c2d),
    })
}

#[cfg(test)]
pub(crate) mod tests {
    //! A minimal client, enough to drive the daemon's half end to end.
    use super::*;
    use ring::signature::{Ed25519KeyPair, KeyPair};

    pub struct Client {
        pub dev: Ed25519KeyPair,
        eph: Option<EphemeralPrivateKey>,
        pub eph_pub: Vec<u8>,
    }

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
            }
        }
        /// `(e2e_finish fields, k_c2d, k_d2c)` for a reply, as the browser computes them.
        pub fn answer(
            &mut self,
            room: &str,
            key: &str,
            reply: &E2eMsg,
            pair: Option<&str>,
        ) -> ((String, String, String), [u8; 32], [u8; 32]) {
            let E2eMsg::E2eReply { eph, host_pk, .. } = reply else {
                panic!()
            };
            let (eph_d, hpk) = (b64(eph).unwrap(), b64(host_pk).unwrap());
            let t1 = transcript(room, key, &self.eph_pub, &eph_d, &hpk);
            let h1 = digest(&SHA256, &t1);
            let (c2d, d2c) = agreement::agree_ephemeral(
                self.eph.take().unwrap(),
                &agreement::UnparsedPublicKey::new(&X25519, &eph_d),
                |s| keys(s, h1.as_ref()),
            )
            .unwrap();
            let mut t2 = t1;
            t2.extend_from_slice(self.dev.public_key().as_ref());
            let h2 = digest(&SHA256, &t2);
            let mac = pair
                .map(|p| {
                    B64.encode(hmac::sign(
                        &hmac::Key::new(hmac::HMAC_SHA256, p.as_bytes()),
                        h2.as_ref(),
                    ))
                })
                .unwrap_or_default();
            (
                (
                    B64.encode(self.dev.public_key()),
                    B64.encode(self.dev.sign(h2.as_ref())),
                    mac,
                ),
                c2d,
                d2c,
            )
        }
    }

    fn host() -> HostKey {
        HostKey::from_seed(&[3u8; 32])
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
        let ((pk, sig, mac), c2d, d2c) = c.answer("room", "ck", &reply_msg, Some("CODE"));
        let codes = vec!["OTHER".to_string(), "CODE".to_string()];
        let mut done = finish(r, &pk, &sig, &mac, &codes).unwrap();
        assert_eq!(done.pair.as_deref(), Some("CODE"));
        // Both directions open.
        let (n, ct) = Sealer::new(&c2d).seal(b"hi");
        assert_eq!(done.opener.open(n, ct).as_deref(), Some(&b"hi"[..]));
        let (n, ct) = done.sealer.seal(b"yo");
        assert_eq!(Opener::new(&d2c).open(n, ct).as_deref(), Some(&b"yo"[..]));
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
        let ((pk, sig, _), ..) = c.answer("room", "attacker", &m, None);
        assert!(finish(r, &pk, &sig, "", &[]).is_err());
        // A wrong pair code proves nothing.
        let mut c = Client::new();
        let (m, r) = reply(&h, "room", "ck", VERSION, &B64.encode(&c.eph_pub)).unwrap();
        let ((pk, sig, mac), ..) = c.answer("room", "ck", &m, Some("GUESS"));
        assert_eq!(
            finish(r, &pk, &sig, &mac, &["CODE".into()]).unwrap().pair,
            None
        );
        // A future version is refused rather than misread.
        assert!(reply(&h, "room", "ck", VERSION + 1, &B64.encode(&c.eph_pub)).is_err());
    }
}
