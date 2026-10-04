//! The end-to-end envelope between a client and its daemon, through the relay
//! (`WADO_PLAN.md`, Decision Log `2026-10-04`).
//!
//! The relay forwards every frame it does not itself speak unread, so these are ordinary text
//! frames to it — no relay change. Handshake, after `join_accepted`:
//!
//! ```text
//! client → daemon  e2e_hello  { v, eph }                     X25519 ephemeral
//! daemon → client  e2e_reply  { eph, host_pk, sig }          sig = Ed25519(host, H(T1))
//! client → daemon  e2e_finish { dev_pk, sig, pair_mac }      sig = Ed25519(device, H(T2))
//! daemon → client  sealed(e2e_ok) — the client is now in; or e2e_fail { reason }
//!
//! T1 = LABEL ‖ lp(room_id) ‖ lp(client_key) ‖ eph_c ‖ eph_d ‖ host_pk
//! T2 = T1 ‖ dev_pk
//! k_c2d ‖ k_d2c = HKDF-SHA256(ikm = X25519(eph), salt = H(T1), info = LABEL), 64 bytes
//! pair_mac = HMAC-SHA256(pair_code, H(T2)) — the pair code itself never leaves the device
//! ```
//!
//! `lp(s)` is a 2-byte big-endian length then the bytes. Everything binary is base64url, no
//! padding. After the handshake every other daemon↔client message is a [`Sealed`] frame:
//! AES-256-GCM over the message's JSON, nonce = 4 zero bytes ‖ the u64 counter big-endian, one
//! counter per direction starting at 0. A counter that is not exactly the next one is an attack
//! or a bug, and either way the link is closed.

use serde::{Deserialize, Serialize};

/// Domain separation for the transcript, the key schedule and the signatures.
pub const LABEL: &[u8] = b"wado-e2e-v1";
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
    },
    E2eFinish {
        dev_pk: String,
        sig: String,
        /// Empty unless the device holds a QR pairing code.
        #[serde(default, skip_serializing_if = "String::is_empty")]
        pair_mac: String,
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
                pair_mac: String::new()
            }
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
