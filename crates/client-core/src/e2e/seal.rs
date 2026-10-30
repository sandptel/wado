//! Sealed frames: AES-256-GCM per direction with a strict counter, as whole wire frames
//! `{"type":"sealed","n":<u64>,"c":"<b64url>"}`. A counter that is not exactly the next one is an
//! attack or a bug, and either way the caller closes the link (`envelope.rs`).

use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use base64::Engine;
use ring::aead::{Aad, LessSafeKey, Nonce, UnboundKey, AES_256_GCM};
use wado_protocol::envelope::{nonce, E2eMsg};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SealError {
    /// Not a sealed frame, or its ciphertext is not base64url.
    Malformed,
    /// Replayed, dropped or reordered.
    OutOfOrder { want: u64, got: u64 },
    /// The tag does not verify: altered, or sealed with another key.
    Auth,
}

fn key(k: &[u8; 32]) -> LessSafeKey {
    LessSafeKey::new(UnboundKey::new(&AES_256_GCM, k).expect("32-byte key"))
}

/// One direction's sending half.
pub struct Sealer {
    key: LessSafeKey,
    n: u64,
}

impl Sealer {
    pub fn new(k: [u8; 32]) -> Self {
        Self { key: key(&k), n: 0 }
    }

    /// The counter the next frame will carry.
    pub fn next_n(&self) -> u64 {
        self.n
    }

    /// The whole wire frame for `plaintext`.
    pub fn seal(&mut self, plaintext: &[u8]) -> String {
        let n = self.n;
        self.n += 1;
        let mut buf = plaintext.to_vec();
        self.key
            .seal_in_place_append_tag(
                Nonce::assume_unique_for_key(nonce(n)),
                Aad::empty(),
                &mut buf,
            )
            .expect("seal cannot fail below 2^64 messages");
        let frame = E2eMsg::Sealed {
            n,
            c: B64.encode(&buf),
        };
        serde_json::to_string(&frame).expect("a sealed frame serialises")
    }
}

/// One direction's receiving half.
pub struct Opener {
    key: LessSafeKey,
    n: u64,
}

impl Opener {
    pub fn new(k: [u8; 32]) -> Self {
        Self { key: key(&k), n: 0 }
    }

    /// The plaintext of a whole wire frame. Any error means the link is no longer trusted.
    pub fn open(&mut self, frame: &str) -> Result<Vec<u8>, SealError> {
        let Ok(E2eMsg::Sealed { n, c }) = serde_json::from_str::<E2eMsg>(frame) else {
            return Err(SealError::Malformed);
        };
        if n != self.n {
            return Err(SealError::OutOfOrder {
                want: self.n,
                got: n,
            });
        }
        let mut buf = B64.decode(c).map_err(|_| SealError::Malformed)?;
        let len = self
            .key
            .open_in_place(
                Nonce::assume_unique_for_key(nonce(n)),
                Aad::empty(),
                &mut buf,
            )
            .map_err(|_| SealError::Auth)?
            .len();
        self.n += 1;
        buf.truncate(len);
        Ok(buf)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::e2e::keys::{hex, session_keys};

    const STOP: &[u8] = br#"{"type":"session_stop"}"#;
    // From `node scripts/e2e-vector.mjs` (and `crates/server/src/e2e/seal.rs` `vector`).
    const CT_HEX: &str =
        "7fa035c9163e20c5ff7922a95b3651312a584ad5c306d45a69d49b882db0b85c875a56044a9be2";

    fn c(frame: &str) -> Vec<u8> {
        match serde_json::from_str::<E2eMsg>(frame).unwrap() {
            E2eMsg::Sealed { c, .. } => B64.decode(c).unwrap(),
            _ => panic!("{frame}"),
        }
    }

    #[test]
    fn vector_seal_session_stop() {
        let (c2d, _) = session_keys(&[7; 32], &[9; 32]);
        let frame = Sealer::new(c2d).seal(STOP);
        assert!(
            frame.starts_with(wado_protocol::envelope::SEALED_PREFIX),
            "{frame}"
        );
        assert!(frame.contains(r#""n":0"#), "{frame}");
        assert_eq!(hex(&c(&frame)), CT_HEX);
    }

    #[test]
    fn opener_rejects_replay() {
        let mut s = Sealer::new([1; 32]);
        let mut o = Opener::new([1; 32]);
        let f0 = s.seal(b"a");
        assert_eq!(o.open(&f0).unwrap(), b"a");
        assert_eq!(o.open(&f0), Err(SealError::OutOfOrder { want: 1, got: 0 }));
    }

    #[test]
    fn opener_rejects_gap() {
        let mut s = Sealer::new([1; 32]);
        let mut o = Opener::new([1; 32]);
        let _f0 = s.seal(b"a");
        let _f1 = s.seal(b"b");
        let f2 = s.seal(b"c");
        assert_eq!(o.open(&f2), Err(SealError::OutOfOrder { want: 0, got: 2 }));
    }

    #[test]
    fn tamper_is_auth_error() {
        let mut s = Sealer::new([1; 32]);
        let mut o = Opener::new([1; 32]);
        let mut ct = c(&s.seal(b"hello"));
        ct[0] ^= 1;
        let bad = serde_json::to_string(&E2eMsg::Sealed {
            n: 0,
            c: B64.encode(&ct),
        })
        .unwrap();
        assert_eq!(o.open(&bad), Err(SealError::Auth));
        assert_eq!(o.open("not json"), Err(SealError::Malformed));
        assert_eq!(o.open(r#"{"type":"e2e_ok"}"#), Err(SealError::Malformed));
    }

    #[test]
    fn roundtrip_with_daemon_opener() {
        let k = [4u8; 32];
        let mut s = Sealer::new(k);
        let mut daemon = wado::e2e::seal::Opener::new(&k);
        for (i, msg) in [&b"one"[..], b"two", STOP].into_iter().enumerate() {
            let frame = s.seal(msg);
            assert_eq!(daemon.open(i as u64, c(&frame)).as_deref(), Some(msg));
        }
        // And the other way: what the daemon seals, this opens.
        let mut ds = wado::e2e::seal::Sealer::new(&k);
        let (n, ct) = ds.seal(b"from the computer");
        let frame = serde_json::to_string(&E2eMsg::Sealed {
            n,
            c: B64.encode(ct),
        })
        .unwrap();
        assert_eq!(Opener::new(k).open(&frame).unwrap(), b"from the computer");
    }
}
