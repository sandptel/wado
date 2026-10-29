//! Message encryption for Web Push (RFC 8291) in the `aes128gcm` content coding (RFC 8188):
//! only the receiver — the browser, or the phone's UnifiedPush app — can read a notification,
//! not the push service that carries it.
//!
//! A fresh P-256 key per message meets the receiver's key (ECDH); HKDF folds in the receiver's
//! auth secret and a random salt; AES-128-GCM seals one record. All from `ring`.

use ring::{aead, agreement, hkdf, rand};

/// One record holds the whole message: push services cap a payload near 4 KB anyway.
const RECORD: u32 = 4096;

struct Len(usize);
impl hkdf::KeyType for Len {
    fn len(&self) -> usize {
        self.0
    }
}

fn expand(prk: &hkdf::Prk, info: &[u8], n: usize) -> Result<Vec<u8>, &'static str> {
    let mut out = vec![0; n];
    prk.expand(&[info], Len(n))
        .and_then(|okm| okm.fill(&mut out))
        .map_err(|_| "hkdf")?;
    Ok(out)
}

/// The deterministic half: everything after the key agreement. `as_public` is this message's
/// own public key, `ua_public` and `auth` the receiver's.
pub fn seal(
    ecdh_secret: &[u8],
    as_public: &[u8],
    ua_public: &[u8],
    auth: &[u8],
    salt: &[u8; 16],
    plaintext: &[u8],
) -> Result<Vec<u8>, &'static str> {
    if as_public.len() != 65 || ua_public.len() != 65 {
        return Err("a push key is not an uncompressed P-256 point");
    }
    // IKM = HKDF(auth, ecdh, "WebPush: info" ‖ 0 ‖ ua_public ‖ as_public, 32)
    let mut key_info = b"WebPush: info\0".to_vec();
    key_info.extend_from_slice(ua_public);
    key_info.extend_from_slice(as_public);
    let prk_key = hkdf::Salt::new(hkdf::HKDF_SHA256, auth).extract(ecdh_secret);
    let ikm = expand(&prk_key, &key_info, 32)?;
    let prk = hkdf::Salt::new(hkdf::HKDF_SHA256, salt).extract(&ikm);
    let cek = expand(&prk, b"Content-Encoding: aes128gcm\0", 16)?;
    let nonce = expand(&prk, b"Content-Encoding: nonce\0", 12)?;

    // One record, the last: the content, then the 0x02 delimiter and no padding.
    let mut body = plaintext.to_vec();
    body.push(2);
    if body.len() + 16 > RECORD as usize {
        return Err("the notification is too long to send");
    }
    let key = aead::LessSafeKey::new(
        aead::UnboundKey::new(&aead::AES_128_GCM, &cek).map_err(|_| "aes key")?,
    );
    let nonce = aead::Nonce::try_assume_unique_for_key(&nonce).map_err(|_| "nonce")?;
    key.seal_in_place_append_tag(nonce, aead::Aad::empty(), &mut body)
        .map_err(|_| "aes-gcm")?;

    // Header: salt ‖ record size ‖ key id length ‖ key id (this message's public key).
    let mut out = Vec::with_capacity(16 + 4 + 1 + 65 + body.len());
    out.extend_from_slice(salt);
    out.extend_from_slice(&RECORD.to_be_bytes());
    out.push(65);
    out.extend_from_slice(as_public);
    out.extend_from_slice(&body);
    Ok(out)
}

/// Encrypt `plaintext` for the receiver with key `ua_public` (65 bytes) and secret `auth`.
pub fn encrypt(ua_public: &[u8], auth: &[u8], plaintext: &[u8]) -> Result<Vec<u8>, &'static str> {
    let rng = rand::SystemRandom::new();
    let me =
        agreement::EphemeralPrivateKey::generate(&agreement::ECDH_P256, &rng).map_err(|_| "key")?;
    let as_public = me.compute_public_key().map_err(|_| "key")?;
    let mut salt = [0u8; 16];
    rand::SecureRandom::fill(&rng, &mut salt).map_err(|_| "rng")?;
    let peer = agreement::UnparsedPublicKey::new(&agreement::ECDH_P256, ua_public);
    let secret = agreement::agree_ephemeral(me, &peer, |s| s.to_vec())
        .map_err(|_| "the receiver's push key is not a valid P-256 key")?;
    seal(
        &secret,
        as_public.as_ref(),
        ua_public,
        auth,
        &salt,
        plaintext,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine as _;

    fn d(s: &str) -> Vec<u8> {
        base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(s)
            .unwrap()
    }

    /// RFC 8291, Appendix A, byte for byte.
    #[test]
    fn rfc8291_appendix_a() {
        let plaintext = d("V2hlbiBJIGdyb3cgdXAsIEkgd2FudCB0byBiZSBhIHdhdGVybWVsb24");
        let as_public = d(
            "BP4z9KsN6nGRTbVYI_c7VJSPQTBtkgcy27mlmlMoZIIgDll6e3vCYLocInmYWAmS6TlzAC8wEqKK6PBru3jl7A8",
        );
        let ua_public = d(
            "BCVxsr7N_eNgVRqvHtD0zTZsEc6-VV-JvLexhqUzORcxaOzi6-AYWXvTBHm4bjyPjs7Vd8pZGH6SRpkNtoIAiw4",
        );
        let salt: [u8; 16] = d("DGv6ra1nlYgDCS1FRnbzlw").try_into().unwrap();
        let auth = d("BTBZMqHH6r4Tts7J_aSIgg");
        let ecdh = d("kyrL1jIIOHEzg3sM2ZWRHDRB62YACZhhSlknJ672kSs");
        let out = seal(&ecdh, &as_public, &ua_public, &auth, &salt, &plaintext).unwrap();
        let mut want = d(
            "DGv6ra1nlYgDCS1FRnbzlwAAEABBBP4z9KsN6nGRTbVYI_c7VJSPQTBtkgcy27mlmlMoZIIgDll6e3vCYLocInmYWAmS6TlzAC8wEqKK6PBru3jl7A8",
        );
        want.extend(d(
            "8pfeW0KbunFT06SuDKoJH9Ql87S1QUrdirN6GcG7sFz1y1sqLgVi1VhjVkHsUoEsbI_0LpXMuGvnzQ",
        ));
        assert_eq!(out, want);
    }

    #[test]
    fn a_fresh_key_and_salt_every_time_and_a_bad_receiver_key_is_refused() {
        let ua = agreement::EphemeralPrivateKey::generate(
            &agreement::ECDH_P256,
            &rand::SystemRandom::new(),
        )
        .unwrap();
        let ua_pub = ua.compute_public_key().unwrap();
        let a = encrypt(ua_pub.as_ref(), &[7; 16], b"hello").unwrap();
        let b = encrypt(ua_pub.as_ref(), &[7; 16], b"hello").unwrap();
        assert_ne!(a, b);
        assert_eq!(a.len(), 86 + 5 + 1 + 16, "header, text, delimiter, tag");
        assert!(encrypt(&[4; 65], &[7; 16], b"hello").is_err());
        assert!(
            encrypt(ua_pub.as_ref(), &[7; 16], &[0; 5000]).is_err(),
            "too long"
        );
    }
}
