//! Whether this computer asks a device for its passkey at all (Decision Log `2026-10-30`: "a
//! passkey must never trap the person"). On for every device unless its owner turns it off —
//! from the computer (`wado passkey require <device> off`, `wado tui`), or from the device itself
//! while it is unlocked. Off, every passkey door (shells, file writes, the trackpad, Claude) is
//! open to it, exactly as if it were unlocked; its grants still decide what it may do at all.
//!
//! On file beside the device's passkey, as `<passkeys>/<device>.off` — the folder that already
//! goes with the device: unpairing deletes it ([`Store::forget`]). Removing only the passkey
//! (`wado passkey remove`) leaves the choice, so a phone whose passkey broke is not shut out by
//! the very step that lets it make a new one.

use serde::{Deserialize, Serialize};

use super::Store;

/// One device the computer does not ask for a passkey.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Off {
    /// Its device key, so a file named by a hash can be read back to whom it belongs.
    pub key: String,
    /// Unix seconds it was turned off.
    #[serde(default)]
    pub since: u64,
}

impl Store {
    // `<name>.off`: a file name never has a dot otherwise, so it cannot be another device's.
    fn off_path(&self, key: &str) -> std::path::PathBuf {
        self.dir()
            .join(format!("{}.off", super::store::file_name(key)))
    }

    /// Is `key` asked for its passkey? Only a look at the folder, cheap enough for every use.
    pub fn required(&self, key: &str) -> bool {
        key.is_empty() || !self.off_path(key).is_file()
    }

    /// Ask `key` for its passkey (`on`), or stop asking it.
    pub fn set_required(&self, key: &str, on: bool) -> std::io::Result<()> {
        if key.is_empty() {
            return Err(std::io::Error::other("no device key"));
        }
        if on {
            return match std::fs::remove_file(self.off_path(key)) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
                _ => Ok(()),
            };
        }
        let off = Off {
            key: key.to_string(),
            since: super::now_s(),
        };
        self.write_file(
            &self.off_path(key),
            &format!("{}.off", super::store::file_name(key)),
            &off,
        )
    }

    /// Every device the computer does not ask, read back from the files themselves.
    pub fn not_required(&self) -> Vec<Off> {
        let Ok(dir) = std::fs::read_dir(self.dir()) else {
            return Vec::new();
        };
        dir.flatten()
            .filter(|e| e.file_name().to_string_lossy().ends_with(".off"))
            .filter_map(|e| std::fs::read_to_string(e.path()).ok())
            .filter_map(|t| serde_json::from_str::<Off>(&t).ok())
            .filter(|o| !o.key.is_empty())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::super::{LOCKED, NO_PASSKEY, refused_with, unlock, verify::Credential};
    use super::*;

    fn temp() -> (std::path::PathBuf, Store) {
        let dir = std::env::temp_dir().join(format!("wado-pk-req-{}", rand::random::<u64>()));
        (dir.clone(), Store::at(dir.join("passkeys")))
    }

    #[test]
    fn off_opens_every_door_and_on_shuts_them_again() {
        let (dir, store) = temp();
        let key = format!("dev-{}", rand::random::<u64>());
        assert!(store.required(&key), "on by default");
        assert_eq!(refused_with(&store, &key, true), Some(NO_PASSKEY));
        store.set_required(&key, false).unwrap();
        assert!(!store.required(&key));
        assert_eq!(
            refused_with(&store, &key, true),
            None,
            "no passkey, still open"
        );
        assert_eq!(refused_with(&store, &key, false), None);
        assert_eq!(store.not_required().len(), 1);
        assert_eq!(store.not_required()[0].key, key);
        let s = super::super::relay::state(&store, &key, "");
        assert!(s.unlocked && !s.required, "the device is told it is open");
        store.set_required(&key, true).unwrap();
        store.set_required(&key, true).unwrap();
        assert_eq!(refused_with(&store, &key, true), Some(NO_PASSKEY));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn removing_the_passkey_keeps_the_choice_unpairing_does_not() {
        let (dir, store) = temp();
        let key = format!("dev-{}", rand::random::<u64>());
        let cred = Credential {
            cred_id: "c".into(),
            key: "k".into(),
            count: 0,
            origin: "https://a.b".into(),
            rp_id: "a.b".into(),
            added: 0,
        };
        store.put(&key, &cred).unwrap();
        assert_eq!(refused_with(&store, &key, true), Some(LOCKED));
        store.set_required(&key, false).unwrap();
        assert_eq!(refused_with(&store, &key, true), None);
        assert!(store.remove(&key));
        assert!(
            !store.required(&key),
            "a broken passkey removed: still not asked"
        );
        store.forget(&key);
        assert!(
            store.required(&key),
            "unpaired: asked again if it ever comes back"
        );
        unlock::lock_in(&store, &key);
        let _ = std::fs::remove_dir_all(dir);
    }
}
