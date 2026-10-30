//! The handshake, client half (wire: [`wado_protocol::envelope`]; the JS twin is
//! `js/e2e_handshake.js`):
//!
//! `join_accepted` → `e2e_hello{eph}` → `e2e_reply{eph, host_pk, sig, sas_commit}` → check the pin
//! and the host's signature over H(T1) **before anything else is sent** → `e2e_finish` → sealed
//! `e2e_ok`. A typed code has one more step: `e2e_sas{nonce}`, which must be the nonce the reply
//! committed to, gives the four check words.

use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use base64::Engine;
use ring::agreement::{self, EphemeralPrivateKey, UnparsedPublicKey, X25519};
use ring::digest::{digest, SHA256};
use ring::rand::SystemRandom;
use ring::signature;
use wado_protocol::envelope::{transcript, E2eMsg, VERSION};

use super::keys::{pair_key, pair_mac, sas_bytes, sas_key, session_keys, typed_mac};
use crate::identity::Identity;
use crate::pins::{Pin, PinFrom};

/// What this device proves about a pairing code, if it holds one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Proof {
    None,
    /// The QR's long code.
    Qr(String),
    /// A typed code, normalised.
    Typed([u8; 8]),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HsError {
    /// The computer's key is not the one pinned for this Remote ID.
    PinMismatch,
    /// The reply's signature over H(T1) does not verify.
    BadSignature,
    /// The check words' nonce is not the one the reply promised.
    SasMismatch,
    Version,
    Malformed,
    Failed(String),
}

/// The link's keys.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Keys {
    pub c2d: [u8; 32],
    pub d2c: [u8; 32],
}

/// A verified reply: the frame to send, the keys, and the computer's pin.
#[derive(Debug)]
pub struct Replied {
    /// The `e2e_finish` frame.
    pub finish: String,
    pub keys: Keys,
    /// base64url(SHA-256(host_pk)).
    pub host_pin: String,
    /// No pin was held: the caller stores `host_pin` as [`PinFrom::FirstUse`].
    pub first_use: bool,
}

struct Typed {
    commit: String,
    k_sas: [u8; 32],
    h2: [u8; 32],
}

pub struct ClientHandshake {
    room_id: String,
    client_key: String,
    eph: Option<EphemeralPrivateKey>,
    eph_pub: Vec<u8>,
    typed: Option<Typed>,
}

fn b64(s: &str) -> Result<Vec<u8>, HsError> {
    B64.decode(s).map_err(|_| HsError::Malformed)
}

fn arr32(d: impl AsRef<[u8]>) -> [u8; 32] {
    let mut o = [0u8; 32];
    o.copy_from_slice(d.as_ref());
    o
}

/// What a computer's public key pins as.
pub fn pin_of(host_pk: &[u8]) -> String {
    B64.encode(digest(&SHA256, host_pk))
}

impl ClientHandshake {
    /// Begin on `join_accepted`: the handshake and its `e2e_hello` frame.
    pub fn start(room_id: &str, client_key: &str) -> (Self, String) {
        let eph = EphemeralPrivateKey::generate(&X25519, &SystemRandom::new()).expect("rng");
        let eph_pub = eph.compute_public_key().expect("rng").as_ref().to_vec();
        let hello = E2eMsg::E2eHello {
            v: VERSION,
            eph: B64.encode(&eph_pub),
        };
        let hs = Self {
            room_id: room_id.to_string(),
            client_key: client_key.to_string(),
            eph: Some(eph),
            eph_pub,
            typed: None,
        };
        (hs, serde_json::to_string(&hello).expect("hello serialises"))
    }

    /// Check an `e2e_reply` and answer it. Nothing is sent unless this returns `Ok`.
    pub fn on_reply(
        &mut self,
        reply_json: &str,
        identity: &Identity,
        pin: Option<&Pin>,
        proof: &Proof,
    ) -> Result<Replied, HsError> {
        let Ok(E2eMsg::E2eReply {
            eph,
            host_pk,
            sig,
            sas_commit,
        }) = serde_json::from_str::<E2eMsg>(reply_json)
        else {
            return Err(HsError::Malformed);
        };
        let (eph_d, hpk, sig) = (b64(&eph)?, b64(&host_pk)?, b64(&sig)?);
        let host_pin = pin_of(&hpk);
        if let Some(p) = pin {
            if p.pin != host_pin {
                return Err(HsError::PinMismatch);
            }
        }
        let t1 = transcript(&self.room_id, &self.client_key, &self.eph_pub, &eph_d, &hpk);
        let h1 = arr32(digest(&SHA256, &t1));
        signature::UnparsedPublicKey::new(&signature::ED25519, &hpk)
            .verify(&h1, &sig)
            .map_err(|_| HsError::BadSignature)?;
        let eph = self.eph.take().ok_or(HsError::Malformed)?;
        let (keys, k_pair, k_sas) =
            agreement::agree_ephemeral(eph, &UnparsedPublicKey::new(&X25519, &eph_d), |shared| {
                let s = arr32(shared);
                let (c2d, d2c) = session_keys(&s, &h1);
                (Keys { c2d, d2c }, pair_key(&s, &h1), sas_key(&s, &h1))
            })
            .map_err(|_| HsError::Malformed)?;
        let mut t2 = t1;
        t2.extend_from_slice(&identity.pub_raw);
        let h2 = arr32(digest(&SHA256, &t2));
        let (pmac, tmac) = match proof {
            Proof::None => (String::new(), String::new()),
            Proof::Qr(code) => (B64.encode(pair_mac(code.as_bytes(), &h2)), String::new()),
            Proof::Typed(code) => {
                self.typed = Some(Typed {
                    commit: sas_commit,
                    k_sas,
                    h2,
                });
                (String::new(), B64.encode(typed_mac(&k_pair, &h2, code)))
            }
        };
        let finish = E2eMsg::E2eFinish {
            dev_pk: B64.encode(identity.pub_raw),
            sig: B64.encode(identity.sign(&h2)),
            pair_mac: pmac,
            typed_mac: tmac,
        };
        Ok(Replied {
            finish: serde_json::to_string(&finish).expect("finish serialises"),
            keys,
            host_pin,
            first_use: pin.is_none(),
        })
    }

    /// The check words' bytes for the computer's `e2e_sas` nonce — only the one it promised.
    pub fn on_sas(&self, nonce_b64: &str) -> Result<[u8; 4], HsError> {
        let t = self.typed.as_ref().ok_or(HsError::Malformed)?;
        let nonce = b64(nonce_b64)?;
        if nonce.len() != 32 || B64.encode(digest(&SHA256, &nonce)) != t.commit {
            return Err(HsError::SasMismatch);
        }
        Ok(sas_bytes(&t.k_sas, &t.h2, &arr32(&nonce)))
    }
}

/// The pin a QR link's `hk` gives, to hold before the first handshake.
pub fn qr_pin(hk: &str) -> Pin {
    Pin {
        pin: hk.to_string(),
        from: PinFrom::Qr,
    }
}
