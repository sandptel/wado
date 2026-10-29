//! The end-to-end envelope between a client and its daemon, through the relay
//! (`WADO_PLAN.md`, Decision Log `2026-10-04`).
//!
//! The relay forwards every frame it does not itself speak unread, so these are ordinary text
//! frames to it — no relay change. Handshake, after `join_accepted`:
//!
//! ```text
//! client → daemon  e2e_hello  { v, eph }                     X25519 ephemeral
//! daemon → client  e2e_reply  { eph, host_pk, sig, sas_commit }  sig = Ed25519(host, H(T1))
//! client → daemon  e2e_finish { dev_pk, sig, pair_mac, typed_mac }  sig = Ed25519(device, H(T2))
//! daemon → client  sealed(e2e_ok) — the client is now in; or e2e_fail { reason }
//!
//! A typed-code pairing has one more step before e2e_ok — the check words (`2026-10-30`):
//!
//! daemon → client  e2e_sas { nonce }        the nonce committed to in e2e_reply
//!   both ends show sas_words(SAS); a person compares them and answers on the computer
//! daemon → client  sealed(e2e_ok) on yes; e2e_fail { reason } on no, or after two minutes
//!
//! T1 = LABEL ‖ lp(room_id) ‖ lp(client_key) ‖ eph_c ‖ eph_d ‖ host_pk
//! T2 = T1 ‖ dev_pk
//! k_c2d ‖ k_d2c = HKDF-SHA256(ikm = X25519(eph), salt = H(T1), info = LABEL), 64 bytes
//! pair_mac = HMAC-SHA256(pair_code, H(T2)) — the pair code itself never leaves the device
//! k_pair = HKDF-SHA256(ikm = X25519(eph), salt = H(T1), info = PAIR_LABEL), 32 bytes
//! typed_mac = HMAC-SHA256(k_pair, H(T2) ‖ typed_code) — typed_code normalised, 8 ASCII bytes
//! sas_commit = SHA-256(nonce), nonce = 32 random bytes, fresh per handshake
//! k_sas = HKDF-SHA256(ikm = X25519(eph), salt = H(T1), info = SAS_LABEL), 32 bytes
//! SAS = HMAC-SHA256(k_sas, H(T2) ‖ nonce)[..4], shown as four words of sas_words.txt
//! ```
//!
//! **Why the check words.** A device pairing by typed code has no pin for the computer yet, so a
//! relay could stand in for the computer to it, learn the short code from its proof by trying
//! all of them offline, and pair a device of its own. Then there are two sessions — the device
//! with the relay, the relay with the computer — and each end's words come from its own. They
//! match only by a 1-in-2³² chance, and the relay cannot steer them: in the session where it is
//! the computer, it has committed to its nonce before it sees anything the device sends after
//! the hello, and before it could know the other session's words, since those need the code
//! (proven once — it is spent then) and the computer's nonce, which it reveals only after the
//! proof. Words from the shared secret alone would not do: the relay picks its ephemeral key
//! last, and could try keys until its words matched.
//!
//! **Why the typed code's proof is keyed differently.** A QR code is 24 random characters, so an
//! HMAC over the public transcript reveals nothing. A typed code is 40 bits: an HMAC over public
//! data alone would let anyone who saw the frame (the relay) try every code offline, past the
//! daemon's attempt limit. Keying it with `k_pair`, which only the two ends of the X25519
//! exchange hold, leaves the relay no oracle — unless it impersonated the computer to a device
//! that had no pin for it yet, which a QR's `hk` pin is what rules out.
//!
//! `lp(s)` is a 2-byte big-endian length then the bytes. Everything binary is base64url, no
//! padding. After the handshake every other daemon↔client message is a [`Sealed`] frame:
//! AES-256-GCM over the message's JSON, nonce = 4 zero bytes ‖ the u64 counter big-endian, one
//! counter per direction starting at 0. A counter that is not exactly the next one is an attack
//! or a bug, and either way the link is closed.

use serde::{Deserialize, Serialize};

/// Domain separation for the transcript, the key schedule and the signatures.
pub const LABEL: &[u8] = b"wado-e2e-v1";
/// HKDF info for `k_pair`, the key a typed pairing code is proven with.
pub const PAIR_LABEL: &[u8] = b"wado-e2e-v1 pair";
/// HKDF info for `k_sas`, the key the check words of a typed pairing come from.
pub const SAS_LABEL: &[u8] = b"wado-e2e-v1 sas";
/// The 256 check words, one per byte. The browser reads the same file.
pub const SAS_WORDS: &str = include_str!("sas_words.txt");
pub const VERSION: u32 = 1;

/// The `type` tag of a sealed frame. Prefix-matched by both ends before any parse.
pub const SEALED_PREFIX: &str = r#"{"type":"sealed""#;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum E2eMsg {
    E2eHello {
        v: u32,
        eph: String,
    },
    E2eReply {
        eph: String,
        host_pk: String,
        sig: String,
        /// SHA-256 of the nonce the check words are made with (see the module docs).
        #[serde(default, skip_serializing_if = "String::is_empty")]
        sas_commit: String,
    },
    E2eFinish {
        dev_pk: String,
        sig: String,
        /// Empty unless the device holds a QR pairing code.
        #[serde(default, skip_serializing_if = "String::is_empty")]
        pair_mac: String,
        /// Empty unless the device was given a typed pairing code.
        #[serde(default, skip_serializing_if = "String::is_empty")]
        typed_mac: String,
    },
    /// Daemon → client, plaintext: the nonce `sas_commit` promised. Sent only after a typed
    /// code's proof checks out; the client shows its check words and waits.
    E2eSas {
        nonce: String,
    },
    /// Daemon → client, sealed: the handshake is done and this device is let in.
    E2eOk,
    /// Daemon → client, plaintext and therefore unauthenticated: shown, never trusted.
    E2eFail {
        reason: String,
    },
    Sealed {
        n: u64,
        c: String,
    },
}

/// The transcript the daemon signs (T1).
pub fn transcript(
    room_id: &str,
    client_key: &str,
    eph_c: &[u8],
    eph_d: &[u8],
    host_pk: &[u8],
) -> Vec<u8> {
    let mut t = LABEL.to_vec();
    for s in [room_id, client_key] {
        t.extend_from_slice(&(s.len() as u16).to_be_bytes());
        t.extend_from_slice(s.as_bytes());
    }
    t.extend_from_slice(eph_c);
    t.extend_from_slice(eph_d);
    t.extend_from_slice(host_pk);
    t
}

/// The four check words for a SAS.
pub fn sas_words(sas: [u8; 4]) -> [&'static str; 4] {
    let word = |b: u8| SAS_WORDS.split_whitespace().nth(b as usize).unwrap_or("?");
    sas.map(word)
}

/// AES-GCM nonce for message `n`.
pub fn nonce(n: u64) -> [u8; 12] {
    let mut out = [0u8; 12];
    out[4..].copy_from_slice(&n.to_be_bytes());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_shapes_are_what_the_browser_writes() {
        let j = |m: &E2eMsg| serde_json::to_string(m).unwrap();
        assert_eq!(
            j(&E2eMsg::E2eHello {
                v: 1,
                eph: "AA".into()
            }),
            r#"{"type":"e2e_hello","v":1,"eph":"AA"}"#
        );
        assert_eq!(j(&E2eMsg::E2eOk), r#"{"type":"e2e_ok"}"#);
        let s = j(&E2eMsg::Sealed {
            n: 3,
            c: "x".into(),
        });
        assert!(s.starts_with(SEALED_PREFIX), "{s}");
        // No pair code: the field is absent, and absent parses back.
        let f: E2eMsg =
            serde_json::from_str(r#"{"type":"e2e_finish","dev_pk":"a","sig":"b"}"#).unwrap();
        assert_eq!(
            f,
            E2eMsg::E2eFinish {
                dev_pk: "a".into(),
                sig: "b".into(),
                pair_mac: String::new(),
                typed_mac: String::new()
            }
        );
    }

    #[test]
    fn there_are_256_distinct_check_words() {
        let w: Vec<&str> = SAS_WORDS.split_whitespace().collect();
        assert_eq!(w.len(), 256);
        let mut d = w.clone();
        d.sort_unstable();
        d.dedup();
        assert_eq!(d.len(), 256, "no word twice");
        assert!(w.iter().all(|w| w.chars().all(|c| c.is_ascii_lowercase())));
        assert_eq!(
            sas_words([0, 1, 255, 2]),
            ["acorn", "anchor", "hatchet", "apple"]
        );
    }

    #[test]
    fn transcript_is_length_prefixed() {
        // "ab"+"c" and "a"+"bc" must not collide.
        assert_ne!(
            transcript("ab", "c", &[], &[], &[]),
            transcript("a", "bc", &[], &[], &[])
        );
        assert_eq!(nonce(1)[11], 1);
        assert_eq!(nonce(1)[..4], [0; 4]);
    }
}
