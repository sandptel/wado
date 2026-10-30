//! The core's keystore box for the app: AES-256-GCM with a 32-byte key the app keeps in the
//! platform's keystore (`flutter_secure_storage`) and hands over once at start. What reaches the
//! disk is only ever sealed by it. Sealed form: a fresh 12-byte nonce, then ciphertext and tag.

use ring::aead::{Aad, LessSafeKey, Nonce, UnboundKey, AES_256_GCM, NONCE_LEN};
use ring::rand::{SecureRandom, SystemRandom};
use wado_client_core::secret::SecretBox;

pub(crate) struct AesBox {
    key: LessSafeKey,
}

impl AesBox {
    pub(crate) fn new(key: &[u8]) -> Result<AesBox, String> {
        if key.len() != 32 {
            return Err("box key must be 32 bytes".into());
        }
        let k = UnboundKey::new(&AES_256_GCM, key).map_err(|_| "bad box key".to_string())?;
        Ok(AesBox {
            key: LessSafeKey::new(k),
        })
    }
}

impl SecretBox for AesBox {
    fn wrap(&self, plain: Vec<u8>) -> Result<Vec<u8>, String> {
        let mut n = [0u8; NONCE_LEN];
        SystemRandom::new()
            .fill(&mut n)
            .map_err(|_| "no randomness".to_string())?;
        let mut buf = plain;
        self.key
            .seal_in_place_append_tag(Nonce::assume_unique_for_key(n), Aad::empty(), &mut buf)
            .map_err(|_| "seal failed".to_string())?;
        let mut out = n.to_vec();
        out.extend(buf);
        Ok(out)
    }

    fn unwrap(&self, sealed: Vec<u8>) -> Result<Vec<u8>, String> {
        if sealed.len() < NONCE_LEN {
            return Err("too short".into());
        }
        let (n, c) = sealed.split_at(NONCE_LEN);
        let mut buf = c.to_vec();
        let nonce = Nonce::try_assume_unique_for_key(n).map_err(|_| "bad nonce".to_string())?;
        let plain = self
            .key
            .open_in_place(nonce, Aad::empty(), &mut buf)
            .map_err(|_| "this phone's key changed".to_string())?;
        Ok(plain.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aes_box_roundtrip() {
        let b = AesBox::new(&[7; 32]).unwrap();
        assert_eq!(
            b.unwrap(b.wrap(b"seed".to_vec()).unwrap()).unwrap(),
            b"seed"
        );
    }

    #[test]
    fn aes_box_rejects_tamper_and_other_keys() {
        let b = AesBox::new(&[7; 32]).unwrap();
        let mut s = b.wrap(b"seed".to_vec()).unwrap();
        assert!(AesBox::new(&[8; 32]).unwrap().unwrap(s.clone()).is_err());
        s[14] ^= 1;
        assert!(b.unwrap(s).is_err());
    }

    #[test]
    fn two_wraps_differ_and_bad_key_length_fails() {
        let b = AesBox::new(&[7; 32]).unwrap();
        assert_ne!(b.wrap(vec![1]).unwrap(), b.wrap(vec![1]).unwrap());
        assert!(AesBox::new(&[1; 31]).is_err());
    }
}
