//! VAPID (RFC 8292): this computer's own key, which a device's subscription is tied to, and the
//! signed token every push carries so the push service knows it came from the key holder.
//!
//! The key is made on first use and kept in `<config>/wado/vapid` (PKCS#8, base64), readable by
//! this user only, shared by every daemon of the pool.

use std::path::{Path, PathBuf};

use base64::Engine as _;
use ring::rand::SystemRandom;
use ring::signature::{ECDSA_P256_SHA256_FIXED_SIGNING, EcdsaKeyPair, KeyPair};

fn b64(b: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(b)
}

/// Who to contact about this sender, as RFC 8292 asks. A placeholder address: wado has no
/// operator, and the push services accept any `mailto:`.
pub const SUBJECT: &str = "mailto:wado@localhost";

/// A token lives this long (RFC 8292 caps it at 24 h).
pub const TTL_S: u64 = 12 * 3600;

pub struct Vapid {
    pair: EcdsaKeyPair,
}

pub fn default_path() -> PathBuf {
    crate::remote_id::config_dir().join("vapid")
}

impl Vapid {
    /// The key at `path`, made there first if there is none.
    pub fn load_or_make(path: &Path) -> Result<Self, String> {
        let rng = SystemRandom::new();
        let pkcs8 = match std::fs::read_to_string(path) {
            Ok(t) => base64::engine::general_purpose::STANDARD
                .decode(t.trim())
                .map_err(|_| {
                    format!(
                        "{} is damaged — delete it to make a new key",
                        path.display()
                    )
                })?,
            Err(_) => {
                let doc = EcdsaKeyPair::generate_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, &rng)
                    .map_err(|_| "could not make a push key")?;
                write_private(
                    path,
                    &base64::engine::general_purpose::STANDARD.encode(doc.as_ref()),
                )
                .map_err(|e| format!("could not save the push key: {e}"))?;
                doc.as_ref().to_vec()
            }
        };
        let pair = EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, &pkcs8, &rng)
            .map_err(|_| {
                format!(
                    "{} is not a push key — delete it to make a new one",
                    path.display()
                )
            })?;
        Ok(Self { pair })
    }

    /// The public key a subscription is made with: uncompressed P-256, base64url.
    pub fn public(&self) -> String {
        b64(self.pair.public_key().as_ref())
    }

    /// `Authorization` for a push to `endpoint`, valid from `now` (unix seconds).
    pub fn header(&self, endpoint: &str, now: u64) -> Result<String, String> {
        Ok(format!(
            "vapid t={}, k={}",
            self.jwt(&audience(endpoint)?, now)?,
            self.public()
        ))
    }

    /// An ES256 JWT for `aud`.
    pub fn jwt(&self, aud: &str, now: u64) -> Result<String, String> {
        let head = b64(br#"{"typ":"JWT","alg":"ES256"}"#);
        let claims = b64(
            serde_json::json!({ "aud": aud, "exp": now + TTL_S, "sub": SUBJECT })
                .to_string()
                .as_bytes(),
        );
        let signing = format!("{head}.{claims}");
        let sig = self
            .pair
            .sign(&SystemRandom::new(), signing.as_bytes())
            .map_err(|_| "could not sign the push token".to_string())?;
        Ok(format!("{signing}.{}", b64(sig.as_ref())))
    }
}

/// `https://host[:port]` of an endpoint: the token's audience.
pub fn audience(endpoint: &str) -> Result<String, String> {
    let rest = endpoint
        .strip_prefix("https://")
        .ok_or("a push endpoint must be https")?;
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    if host.is_empty() || host.contains('@') {
        return Err("that push endpoint has no host".into());
    }
    Ok(format!("https://{host}"))
}

fn write_private(path: &Path, text: &str) -> std::io::Result<()> {
    use std::io::Write as _;
    use std::os::unix::fs::OpenOptionsExt as _;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut f = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(path)?;
    f.write_all(text.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ring::signature::{ECDSA_P256_SHA256_FIXED, UnparsedPublicKey};

    fn d(s: &str) -> Vec<u8> {
        base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(s)
            .unwrap()
    }

    #[test]
    fn the_token_is_an_es256_jwt_for_the_push_service_signed_by_this_key() {
        let path = std::env::temp_dir().join(format!("wado-vapid-{}", rand::random::<u64>()));
        let v = Vapid::load_or_make(&path).unwrap();
        let again = Vapid::load_or_make(&path).unwrap();
        assert_eq!(v.public(), again.public(), "made once, then kept");
        assert_eq!(d(&v.public()).len(), 65);
        let h = v
            .header("https://fcm.googleapis.com/fcm/send/abc:def", 1_000)
            .unwrap();
        let (t, k) = h
            .strip_prefix("vapid t=")
            .unwrap()
            .split_once(", k=")
            .unwrap();
        assert_eq!(k, v.public());
        let parts: Vec<&str> = t.split('.').collect();
        assert_eq!(parts.len(), 3);
        let head: serde_json::Value = serde_json::from_slice(&d(parts[0])).unwrap();
        assert_eq!(head["alg"], "ES256");
        let claims: serde_json::Value = serde_json::from_slice(&d(parts[1])).unwrap();
        assert_eq!(claims["aud"], "https://fcm.googleapis.com");
        assert_eq!(claims["exp"], 1_000 + TTL_S);
        assert!(claims["sub"].as_str().unwrap().starts_with("mailto:"));
        let sig = d(parts[2]);
        assert_eq!(sig.len(), 64, "JWS ES256 is r‖s, not DER");
        UnparsedPublicKey::new(&ECDSA_P256_SHA256_FIXED, d(&v.public()))
            .verify(format!("{}.{}", parts[0], parts[1]).as_bytes(), &sig)
            .expect("verifies with the public key the subscription was made with");
        use std::os::unix::fs::PermissionsExt as _;
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o077,
            0
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn audiences() {
        assert_eq!(
            audience("https://updates.push.services.mozilla.com/wpush/v2/x").unwrap(),
            "https://updates.push.services.mozilla.com"
        );
        assert_eq!(
            audience("https://ntfy.example:8443/up?x").unwrap(),
            "https://ntfy.example:8443"
        );
        assert!(audience("http://plain.example/x").is_err());
        assert!(audience("https://user@evil/x").is_err());
    }
}
