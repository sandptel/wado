//! AES-256-GCM per direction, with a strict counter: replayed, dropped or reordered frames
//! fail to open, and the caller closes the link.

use ring::aead::{AES_256_GCM, Aad, LessSafeKey, Nonce, UnboundKey};
use ring::hkdf;
use wado_protocol::envelope::{LABEL, nonce};

pub struct Sealer {
    key: LessSafeKey,
    n: u64,
}

pub struct Opener {
    key: LessSafeKey,
    n: u64,
}

fn key(k: &[u8; 32]) -> LessSafeKey {
    LessSafeKey::new(UnboundKey::new(&AES_256_GCM, k).expect("32-byte key"))
}

impl Sealer {
    pub fn new(k: &[u8; 32]) -> Self {
        Self { key: key(k), n: 0 }
    }
    /// `(counter, ciphertext ‖ tag)`.
    pub fn seal(&mut self, plain: &[u8]) -> (u64, Vec<u8>) {
        let n = self.n;
        self.n += 1;
        let mut buf = plain.to_vec();
        self.key
            .seal_in_place_append_tag(
                Nonce::assume_unique_for_key(nonce(n)),
                Aad::empty(),
                &mut buf,
            )
            .expect("seal cannot fail below 2^64 messages");
        (n, buf)
    }
}

impl Opener {
    pub fn new(k: &[u8; 32]) -> Self {
        Self { key: key(k), n: 0 }
    }
    /// `None` for a wrong counter or a failed tag. Either way the link is no longer trusted.
    pub fn open(&mut self, n: u64, mut c: Vec<u8>) -> Option<Vec<u8>> {
        if n != self.n {
            return None;
        }
        let len = self
            .key
            .open_in_place(Nonce::assume_unique_for_key(nonce(n)), Aad::empty(), &mut c)
            .ok()?
            .len();
        self.n += 1;
        c.truncate(len);
        Some(c)
    }
}

struct Len(usize);
impl hkdf::KeyType for Len {
    fn len(&self) -> usize {
        self.0
    }
}

/// `(k_c2d, k_d2c)` from the X25519 shared secret and H(T1).
pub fn keys(shared: &[u8], salt: &[u8]) -> ([u8; 32], [u8; 32]) {
    let mut out = [0u8; 64];
    hkdf::Salt::new(hkdf::HKDF_SHA256, salt)
        .extract(shared)
        .expand(&[LABEL], Len(64))
        .and_then(|okm| okm.fill(&mut out))
        .expect("64 bytes is within HKDF-SHA256's limit");
    let (mut a, mut b) = ([0u8; 32], [0u8; 32]);
    a.copy_from_slice(&out[..32]);
    b.copy_from_slice(&out[32..]);
    (a, b)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(b: &[u8]) -> String {
        b.iter().map(|x| format!("{x:02x}")).collect()
    }

    /// The shared vector: `scripts/e2e-vector.mjs` checks the browser side computes the same.
    #[test]
    fn vector() {
        let (c2d, d2c) = keys(&[7u8; 32], &[9u8; 32]);
        let (n, c) = Sealer::new(&c2d).seal(b"{\"type\":\"session_stop\"}");
        assert_eq!(n, 0);
        assert_eq!(
            hex(&c2d),
            "3333fc281be52212d97b855583beacebcc8775e996ca1ce69d1dc46f6d778255"
        );
        assert_eq!(
            hex(&d2c),
            "37a538fc09ba3027b0137fcdd446183998eba633f241367567d8a56b0b34f7e2"
        );
        assert_eq!(
            hex(&c),
            "7fa035c9163e20c5ff7922a95b3651312a584ad5c306d45a69d49b882db0b85c875a56044a9be2"
        );
    }

    #[test]
    fn round_trip_and_strict_counter() {
        let k = [1u8; 32];
        let mut s = Sealer::new(&k);
        let mut o = Opener::new(&k);
        let (n0, c0) = s.seal(b"a");
        let (n1, c1) = s.seal(b"b");
        assert_eq!(o.open(n1, c1.clone()), None, "skipping ahead is refused");
        assert_eq!(o.open(n0, c0.clone()).as_deref(), Some(&b"a"[..]));
        assert_eq!(o.open(n0, c0), None, "a replay is refused");
        let mut bad = c1.clone();
        bad[0] ^= 1;
        assert_eq!(o.open(n1, bad), None, "a flipped byte is refused");
    }
}
