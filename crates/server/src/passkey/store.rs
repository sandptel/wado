//! Each device's passkey, on file beside the trust list: `<config>/wado/passkeys/<device key>`,
//! JSON, readable by this user only. Shared by every daemon of the pool, like the trust list.

use std::path::PathBuf;

use super::verify::Credential;

#[derive(Clone)]
pub struct Store {
    dir: PathBuf,
}

impl Default for Store {
    fn default() -> Self {
        Self::at(crate::remote_id::config_dir().join("passkeys"))
    }
}

/// A device key as a file name: itself when it is plainly safe, else its SHA-256 in hex.
fn file_name(key: &str) -> String {
    let plain = !key.is_empty()
        && key.len() <= 128
        && key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if plain {
        return key.to_string();
    }
    ring::digest::digest(&ring::digest::SHA256, key.as_bytes())
        .as_ref()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

impl Store {
    pub fn at(dir: PathBuf) -> Self {
        Self { dir }
    }

    fn path(&self, key: &str) -> PathBuf {
        self.dir.join(file_name(key))
    }

    pub fn get(&self, key: &str) -> Option<Credential> {
        if key.is_empty() {
            return None;
        }
        let text = std::fs::read_to_string(self.path(key)).ok()?;
        serde_json::from_str(&text).ok()
    }

    /// Write `key`'s passkey, replacing any it had. Atomic: a crash leaves the old one or the new.
    pub fn put(&self, key: &str, cred: &Credential) -> std::io::Result<()> {
        use std::io::Write as _;
        use std::os::unix::fs::OpenOptionsExt as _;
        std::fs::create_dir_all(&self.dir)?;
        let tmp = self.dir.join(format!(".{}.tmp", file_name(key)));
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .mode(0o600)
            .open(&tmp)?;
        f.write_all(serde_json::to_string_pretty(cred)?.as_bytes())?;
        f.sync_all()?;
        std::fs::rename(tmp, self.path(key))
    }

    /// The counter after an unlock.
    pub fn set_count(&self, key: &str, count: u32) {
        if let Some(mut c) = self.get(key) {
            c.count = count;
            if let Err(e) = self.put(key, &c) {
                tracing::warn!("passkey: could not save the counter: {e}");
            }
        }
    }

    /// Forget `key`'s passkey. True if there was one.
    pub fn remove(&self, key: &str) -> bool {
        std::fs::remove_file(self.path(key)).is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cred() -> Credential {
        Credential {
            cred_id: "abc".into(),
            key: "BA".into(),
            count: 3,
            origin: "https://a.b".into(),
            rp_id: "a.b".into(),
            added: 1,
        }
    }

    #[test]
    fn put_get_count_remove() {
        let dir = std::env::temp_dir().join(format!("wado-passkeys-{}", rand::random::<u64>()));
        let s = Store::at(dir.clone());
        assert_eq!(s.get("phone"), None);
        s.put("phone", &cred()).unwrap();
        assert_eq!(s.get("phone"), Some(cred()));
        s.set_count("phone", 9);
        assert_eq!(s.get("phone").unwrap().count, 9);
        use std::os::unix::fs::PermissionsExt as _;
        let mode = std::fs::metadata(dir.join("phone"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o077, 0, "nobody else may read it");
        assert!(s.remove("phone"));
        assert!(!s.remove("phone"));
        assert_eq!(s.get(""), None);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_key_never_escapes_the_folder() {
        assert_eq!(file_name("Phone-key_1"), "Phone-key_1");
        let odd = file_name("../../etc/passwd");
        assert!(!odd.contains('/') && odd.len() == 64, "{odd}");
    }
}
