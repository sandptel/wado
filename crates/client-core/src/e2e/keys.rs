//! The envelope's key schedule, pure: HKDF-SHA256 with `salt = H(T1)` and the labels of
//! [`wado_protocol::envelope`], and the HMACs a pairing proof and the check words are made of.
//! The daemon's twin is `crates/server/src/e2e/seal.rs`; `scripts/e2e-vector.mjs` pins the
//! browser's to the same bytes.

use ring::{hkdf, hmac};
use wado_protocol::envelope::{LABEL, PAIR_LABEL, SAS_LABEL};

struct Len(usize);
impl hkdf::KeyType for Len {
    fn len(&self) -> usize {
        self.0
    }
}

fn expand(shared: &[u8], salt: &[u8], info: &[u8], out: &mut [u8]) {
    hkdf::Salt::new(hkdf::HKDF_SHA256, salt)
        .extract(shared)
        .expand(&[info], Len(out.len()))
        .and_then(|okm| okm.fill(out))
        .expect("at most 64 bytes is within HKDF-SHA256's limit");
}

fn mac(key: &[u8], parts: &[&[u8]]) -> [u8; 32] {
    let mut ctx = hmac::Context::with_key(&hmac::Key::new(hmac::HMAC_SHA256, key));
    for p in parts {
        ctx.update(p);
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(ctx.sign().as_ref());
    out
}

/// `(k_c2d, k_d2c)` from the X25519 shared secret and H(T1).
pub fn session_keys(shared: &[u8; 32], salt: &[u8; 32]) -> ([u8; 32], [u8; 32]) {
    let mut out = [0u8; 64];
    expand(shared, salt, LABEL, &mut out);
    let (mut c2d, mut d2c) = ([0u8; 32], [0u8; 32]);
    c2d.copy_from_slice(&out[..32]);
    d2c.copy_from_slice(&out[32..]);
    (c2d, d2c)
}

/// `k_pair`: what a typed pairing code is proven with.
pub fn pair_key(shared: &[u8; 32], salt: &[u8; 32]) -> [u8; 32] {
    let mut out = [0u8; 32];
    expand(shared, salt, PAIR_LABEL, &mut out);
    out
}

/// `k_sas`: what a typed pairing's check words come from.
pub fn sas_key(shared: &[u8; 32], salt: &[u8; 32]) -> [u8; 32] {
    let mut out = [0u8; 32];
    expand(shared, salt, SAS_LABEL, &mut out);
    out
}

/// A QR code's proof: `HMAC(pair_code, H(T2))`. The code itself never leaves the device.
pub fn pair_mac(pair_code: &[u8], h_t2: &[u8; 32]) -> [u8; 32] {
    mac(pair_code, &[h_t2])
}

/// A typed code's proof: `HMAC(k_pair, H(T2) ‖ code)`, the code normalised.
pub fn typed_mac(k_pair: &[u8; 32], h_t2: &[u8; 32], code8: &[u8; 8]) -> [u8; 32] {
    mac(k_pair, &[h_t2, code8])
}

/// The check words' four bytes: `HMAC(k_sas, H(T2) ‖ nonce)[..4]`.
pub fn sas_bytes(k_sas: &[u8; 32], h_t2: &[u8; 32], nonce: &[u8; 32]) -> [u8; 4] {
    let tag = mac(k_sas, &[h_t2, nonce]);
    [tag[0], tag[1], tag[2], tag[3]]
}

#[cfg(test)]
pub(crate) fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // From `node scripts/e2e-vector.mjs` — the same bytes `crates/server/src/e2e/seal.rs` pins.
    const C2D_HEX: &str = "3333fc281be52212d97b855583beacebcc8775e996ca1ce69d1dc46f6d778255";
    const D2C_HEX: &str = "37a538fc09ba3027b0137fcdd446183998eba633f241367567d8a56b0b34f7e2";
    const PAIR_HEX: &str = "a9b2a7e836c7ae6a4ae6b4fa35d020280d1ba01f03c2c6e93753e4120fc6a18c";
    const TYPED_HEX: &str = "b1ae135713327fcf863f9cb8d7f8cfdc26c6fd99fb7c72cda383e7fcfe207447";
    const SAS_KEY_HEX: &str = "fcd6888ab08b88c8073ffd51b268af29878f82fef676f19bd1c0a7b1e3f0b51e";
    const SAS_WORDS: [&str; 4] = ["desert", "badger", "torch", "shell"];

    #[test]
    fn vector_session_keys() {
        let (c2d, d2c) = session_keys(&[7; 32], &[9; 32]);
        assert_eq!(hex(&c2d), C2D_HEX);
        assert_eq!(hex(&d2c), D2C_HEX);
    }

    #[test]
    fn vector_typed_mac() {
        let k = pair_key(&[7; 32], &[9; 32]);
        assert_eq!(hex(&k), PAIR_HEX);
        assert_eq!(hex(&typed_mac(&k, &[5; 32], b"K7QF2MXP")), TYPED_HEX);
    }

    #[test]
    fn vector_sas_bytes() {
        // As e2e-vector.mjs's sas section: H(T2) = 32 × 5, nonce = 32 × 6.
        let k = sas_key(&[7; 32], &[9; 32]);
        assert_eq!(hex(&k), SAS_KEY_HEX);
        let sas = sas_bytes(&k, &[5; 32], &[6; 32]);
        assert_eq!(wado_protocol::envelope::sas_words(sas), SAS_WORDS);
    }

    #[test]
    fn pair_mac_is_hmac_of_the_transcript_hash() {
        // Keyed by the code, over H(T2): what the daemon's `finish` verifies.
        let a = pair_mac(b"CODE", &[1; 32]);
        assert_ne!(a, pair_mac(b"CODF", &[1; 32]));
        assert_ne!(a, pair_mac(b"CODE", &[2; 32]));
    }
}
