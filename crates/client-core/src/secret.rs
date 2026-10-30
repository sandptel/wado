//! The platform's keystore, as the core sees it: wrap and unwrap a secret. On Android it is an
//! AndroidKeyStore AES-GCM key (`KeystoreSecretBox.kt`); the core never holds a key of its own
//! for this, and nothing it wraps is ever written unwrapped.

pub trait SecretBox: Send + Sync {
    fn wrap(&self, plain: Vec<u8>) -> Result<Vec<u8>, String>;
    fn unwrap(&self, sealed: Vec<u8>) -> Result<Vec<u8>, String>;
}

/// A stand-in for tests: XOR with a fixed byte. No protection — it only proves that what reaches
/// the disk went through the box.
#[cfg(test)]
pub struct XorBox(pub u8);

#[cfg(test)]
impl SecretBox for XorBox {
    fn wrap(&self, plain: Vec<u8>) -> Result<Vec<u8>, String> {
        Ok(plain.into_iter().map(|b| b ^ self.0).collect())
    }
    fn unwrap(&self, sealed: Vec<u8>) -> Result<Vec<u8>, String> {
        self.wrap(sealed)
    }
}
