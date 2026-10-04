//! This computer's long-term identity: one Ed25519 key in the shared config dir, so every
//! daemon of a pool presents the same one and a device pins it once.

use std::fs;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use ring::rand::SystemRandom;
use ring::signature::{Ed25519KeyPair, KeyPair};
use tracing::info;

pub struct HostKey {
    pair: Ed25519KeyPair,
}

impl HostKey {
    /// Load `<dir>/host_key`, or make it (0600). `create_new` makes two daemons of a pool
    /// starting at once agree: the loser reads the winner's key.
    pub fn load_or_create(dir: &Path) -> crate::Result<Self> {
        let path = dir.join("host_key");
        let err = |e: &dyn std::fmt::Display| crate::WadoError::Other(format!("host_key: {e}"));
        if !path.exists() {
            let doc = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).map_err(|e| err(&e))?;
            fs::create_dir_all(dir).map_err(|e| err(&e))?;
            match fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&path)
            {
                Ok(mut f) => {
                    f.write_all(doc.as_ref()).map_err(|e| err(&e))?;
                    info!(
                        "e2e: made this computer's identity key at {}",
                        path.display()
                    );
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(e) => return Err(err(&e)),
            }
        }
        let bytes = fs::read(&path).map_err(|e| err(&e))?;
        let pair = Ed25519KeyPair::from_pkcs8(&bytes).map_err(|e| err(&e))?;
        Ok(Self { pair })
    }

    pub fn public(&self) -> &[u8] {
        self.pair.public_key().as_ref()
    }

    /// What the QR carries and a device pins: base64url(SHA-256(public key)).
    pub fn pin(&self) -> String {
        pin_of(self.public())
    }

    pub fn sign(&self, msg: &[u8]) -> Vec<u8> {
        self.pair.sign(msg).as_ref().to_vec()
    }

    #[cfg(test)]
    pub fn from_seed(seed: &[u8; 32]) -> Self {
        Self {
            pair: Ed25519KeyPair::from_seed_unchecked(seed).unwrap(),
        }
    }
}

pub fn pin_of(pk: &[u8]) -> String {
    B64.encode(ring::digest::digest(&ring::digest::SHA256, pk))
}

#[cfg(test)]
mod tests {
    use super::HostKey;

    #[test]
    fn made_once_then_reloaded_private() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("wado-hk-{}", rand::random::<u64>()));
        let a = HostKey::load_or_create(&dir).unwrap();
        let b = HostKey::load_or_create(&dir).unwrap();
        assert_eq!(a.public(), b.public());
        let mode = std::fs::metadata(dir.join("host_key"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
        let _ = std::fs::remove_dir_all(dir);
    }
}
