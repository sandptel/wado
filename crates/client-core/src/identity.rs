//! This device: an Ed25519 key and the random `client_key` the relay and the computer's trust list
//! know it by. Made once; the seed is written only wrapped by the platform's [`SecretBox`], in
//! `identity.bin`. A new identity is a new device to every computer (re-pair).

use std::path::Path;

use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use base64::Engine;
use ring::rand::{SecureRandom, SystemRandom};
use ring::signature::{Ed25519KeyPair, KeyPair};
use serde::{Deserialize, Serialize};

use crate::secret::SecretBox;
use crate::store;

const FILE: &str = "identity.bin";

#[derive(Serialize, Deserialize)]
struct Stored {
    seed: String,
    client_key: String,
}

/// 16 random bytes as a v4 UUID: `xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx`.
fn uuid(mut b: [u8; 16]) -> String {
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let h: String = b.iter().map(|x| format!("{x:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        &h[0..8],
        &h[8..12],
        &h[12..16],
        &h[16..20],
        &h[20..32]
    )
}

pub struct Identity {
    /// A random UUID (v4 form, as the browser's `crypto.randomUUID()`): the seat the relay holds
    /// and the line on the trust list. The relay keeps only `[A-Za-z0-9-]` of it (`join.rs`), so
    /// base64url would lose its `_` there and break the handshake's transcript.
    pub client_key: String,
    /// The raw 32-byte public key the computer pins this device by.
    pub pub_raw: [u8; 32],
    pair: Ed25519KeyPair,
}

impl Identity {
    /// Load this device's identity from `dir`, or make it.
    pub fn load_or_create(dir: &Path, sb: &dyn SecretBox) -> Result<Identity, String> {
        if let Some(sealed) = store::read(dir, FILE) {
            let plain = sb.unwrap(sealed)?;
            let s: Stored = serde_json::from_slice(&plain).map_err(|e| format!("identity: {e}"))?;
            let seed = B64
                .decode(&s.seed)
                .map_err(|_| "identity: bad seed".to_string())?;
            return Self::from_parts(&seed, s.client_key);
        }
        let rng = SystemRandom::new();
        let (mut seed, mut ck) = ([0u8; 32], [0u8; 16]);
        rng.fill(&mut seed).map_err(|_| "rng")?;
        rng.fill(&mut ck).map_err(|_| "rng")?;
        let s = Stored {
            seed: B64.encode(seed),
            client_key: uuid(ck),
        };
        let plain = serde_json::to_vec(&s).map_err(|e| e.to_string())?;
        let sealed = sb.wrap(plain)?;
        store::write(dir, FILE, &sealed).map_err(|e| format!("identity: {e}"))?;
        Self::from_parts(&seed, s.client_key)
    }

    fn from_parts(seed: &[u8], client_key: String) -> Result<Identity, String> {
        let pair = Ed25519KeyPair::from_seed_unchecked(seed)
            .map_err(|_| "identity: bad seed".to_string())?;
        let mut pub_raw = [0u8; 32];
        pub_raw.copy_from_slice(pair.public_key().as_ref());
        Ok(Identity {
            client_key,
            pub_raw,
            pair,
        })
    }

    pub fn sign(&self, msg: &[u8]) -> [u8; 64] {
        let mut out = [0u8; 64];
        out.copy_from_slice(self.pair.sign(msg).as_ref());
        out
    }

    /// A throwaway identity (tests, and a second core in the live test).
    pub fn ephemeral() -> Identity {
        let mut seed = [0u8; 32];
        let mut ck = [0u8; 16];
        let rng = SystemRandom::new();
        rng.fill(&mut seed).expect("rng");
        rng.fill(&mut ck).expect("rng");
        Self::from_parts(&seed, uuid(ck)).expect("a fresh seed")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secret::XorBox;

    #[test]
    fn identity_is_stable_across_loads() {
        let d = store::temp_dir("id");
        let a = Identity::load_or_create(&d, &XorBox(0x5a)).unwrap();
        let b = Identity::load_or_create(&d, &XorBox(0x5a)).unwrap();
        assert_eq!(a.client_key, b.client_key);
        assert_eq!(a.pub_raw, b.pub_raw);
        assert_eq!(a.sign(b"x"), b.sign(b"x"));
        assert_eq!(a.client_key.len(), 36, "a UUID");
        assert!(a
            .client_key
            .chars()
            .all(|c| c.is_ascii_hexdigit() || c == '-'));
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn identity_seed_never_written_unwrapped() {
        let d = store::temp_dir("id-raw");
        let _ = Identity::load_or_create(&d, &XorBox(0x5a)).unwrap();
        // The seed, as it is held in memory before wrapping (base64url of the raw bytes).
        let plain = XorBox(0x5a).unwrap(store::read(&d, FILE).unwrap()).unwrap();
        let s: Stored = serde_json::from_slice(&plain).unwrap();
        let seed = B64.decode(&s.seed).unwrap();
        for entry in std::fs::read_dir(&d).unwrap() {
            let bytes = std::fs::read(entry.unwrap().path()).unwrap();
            assert!(!contains(&bytes, &seed), "raw seed on disk");
            assert!(!contains(&bytes, s.seed.as_bytes()), "encoded seed on disk");
        }
        let _ = std::fs::remove_dir_all(d);
    }

    fn contains(hay: &[u8], needle: &[u8]) -> bool {
        hay.windows(needle.len()).any(|w| w == needle)
    }
}
