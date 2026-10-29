//! AES-256-GCM per direction, with a strict counter: replayed, dropped or reordered frames
//! fail to open, and the caller closes the link.

use ring::aead::{AES_256_GCM, Aad, LessSafeKey, Nonce, UnboundKey};
use ring::hkdf;
use wado_protocol::envelope::{LABEL, PAIR_LABEL, nonce};

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

/// `k_pair` — what a typed pairing code is proven with — from the same secret and salt.
pub fn pair_key(shared: &[u8], salt: &[u8]) -> [u8; 32] {
    let mut out = [0u8; 32];
    hkdf::Salt::new(hkdf::HKDF_SHA256, salt)
        .extract(shared)
        .expand(&[PAIR_LABEL], Len(32))
        .and_then(|okm| okm.fill(&mut out))
        .expect("32 bytes is within HKDF-SHA256's limit");
    out
}

/// The typed code's proof: `HMAC(k_pair, H(T2) ‖ code)`.
pub fn typed_mac(k_pair: &[u8; 32], h2: &[u8], code: &str) -> ring::hmac::Tag {
    let mut ctx =
        ring::hmac::Context::with_key(&ring::hmac::Key::new(ring::hmac::HMAC_SHA256, k_pair));
    ctx.update(h2);
    ctx.update(code.as_bytes());
    ctx.sign()
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

    /// The typed code's key and proof: `scripts/e2e-vector.mjs` pins the same bytes.
    #[test]
    fn pair_vector() {
        let k = pair_key(&[7u8; 32], &[9u8; 32]);
        assert_eq!(
            hex(&k),
            "a9b2a7e836c7ae6a4ae6b4fa35d020280d1ba01f03c2c6e93753e4120fc6a18c"
        );
        assert_eq!(
            hex(typed_mac(&k, &[5u8; 32], "K7QF2MXP").as_ref()),
            "b1ae135713327fcf863f9cb8d7f8cfdc26c6fd99fb7c72cda383e7fcfe207447"
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
