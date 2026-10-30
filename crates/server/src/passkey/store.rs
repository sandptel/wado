//! Each device's passkey, on file beside the trust list: `<config>/wado/passkeys/<device key>`,
//! JSON, readable by this user only. Shared by every daemon of the pool, like the trust list.

use std::path::PathBuf;

use super::stay::Prefs;
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
pub(super) fn file_name(key: &str) -> String {
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

    /// The folder itself, for the files that live beside a passkey (`super::require`).
    pub(super) fn dir(&self) -> &std::path::Path {
        &self.dir
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
        self.write_file(&self.path(key), &file_name(key), cred)
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

    /// The trust list beside this folder.
    pub fn gate(&self) -> crate::gate::Gate {
        crate::gate::Gate::at(self.dir.parent().map(PathBuf::from).unwrap_or_default())
    }

    /// Is a passkey on file for `key`? Only a look at the folder, cheap enough for every use.
    pub fn has(&self, key: &str) -> bool {
        !key.is_empty() && self.path(key).is_file()
    }

    /// Forget `key`'s passkey, and with it how long its unlock lasts. True if there was one.
    /// Whether it is asked for one at all stays: that is the owner's choice, not the passkey's.
    pub fn remove(&self, key: &str) -> bool {
        let _ = std::fs::remove_file(self.prefs_path(key));
        std::fs::remove_file(self.path(key)).is_ok()
    }

    /// Forget everything about `key` here — the device was unpaired. True if it had a passkey.
    pub fn forget(&self, key: &str) -> bool {
        let _ = self.set_required(key, true);
        self.remove(key)
    }

    // `<name>.stay`: a file name never has a dot otherwise, so it cannot be another device's.
    fn prefs_path(&self, key: &str) -> PathBuf {
        self.dir.join(format!("{}.stay", file_name(key)))
    }

    /// How long `key`'s unlock lasts ([`Prefs`]); the default when it never chose.
    pub fn prefs(&self, key: &str) -> Prefs {
        if key.is_empty() {
            return Prefs::default();
        }
        std::fs::read_to_string(self.prefs_path(key))
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    /// Write `key`'s choice. Only for a device with a passkey here: the choice goes with it.
    pub fn set_prefs(&self, key: &str, p: &Prefs) -> std::io::Result<()> {
        if !self.has(key) {
            return Err(std::io::Error::other("no passkey"));
        }
        let p = Prefs {
            key: key.to_string(),
            ..p.clone()
        };
        self.write_file(
            &self.prefs_path(key),
            &format!("{}.stay", file_name(key)),
            &p,
        )
    }

    /// Every device whose choice is on file, read back from the files themselves.
    pub fn all_prefs(&self) -> Vec<Prefs> {
        let Ok(dir) = std::fs::read_dir(&self.dir) else {
            return Vec::new();
        };
        dir.flatten()
            .filter(|e| e.file_name().to_string_lossy().ends_with(".stay"))
            .filter_map(|e| std::fs::read_to_string(e.path()).ok())
            .filter_map(|t| serde_json::from_str::<Prefs>(&t).ok())
            .filter(|p| !p.key.is_empty())
            .collect()
    }

    pub(super) fn write_file<T: serde::Serialize>(
        &self,
        to: &std::path::Path,
        name: &str,
        v: &T,
    ) -> std::io::Result<()> {
        use std::io::Write as _;
        use std::os::unix::fs::OpenOptionsExt as _;
        std::fs::create_dir_all(&self.dir)?;
        let tmp = self.dir.join(format!(".{name}.tmp"));
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .mode(0o600)
            .open(&tmp)?;
        f.write_all(serde_json::to_string_pretty(v)?.as_bytes())?;
        f.sync_all()?;
        std::fs::rename(tmp, to)
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
    fn the_choice_lives_and_dies_with_the_passkey() {
        let dir = std::env::temp_dir().join(format!("wado-passkeys-{}", rand::random::<u64>()));
        let s = Store::at(dir.clone());
        let mut p = Prefs {
            stay: "lock".into(),
            lock_on_leave: false,
            ..Prefs::default()
        };
        assert!(s.set_prefs("phone", &p).is_err(), "no passkey, no choice");
        assert_eq!(s.prefs("phone"), Prefs::default());
        s.put("phone", &cred()).unwrap();
        assert!(s.has("phone") && !s.has("tablet") && !s.has(""));
        p.open_since = Some(5);
        s.set_prefs("phone", &p).unwrap();
        let back = s.prefs("phone");
        assert_eq!((back.stay.as_str(), back.lock_on_leave), ("lock", false));
        assert_eq!(back.key, "phone", "it says whose it is");
        assert_eq!(s.all_prefs().len(), 1);
        assert!(s.remove("phone"));
        assert_eq!(
            s.prefs("phone"),
            Prefs::default(),
            "removed with the passkey"
        );
        assert!(s.all_prefs().is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_key_never_escapes_the_folder() {
        assert_eq!(file_name("Phone-key_1"), "Phone-key_1");
        let odd = file_name("../../etc/passwd");
        assert!(!odd.contains('/') && odd.len() == 64, "{odd}");
    }
}
