//! The computers this device has paired with, in `profiles.json`. Written whole and atomically
//! ([`crate::store`]), and only after a pairing's `e2e_ok` — a pairing cut short leaves nothing.
//! The OS fields are a cache of the computer's last `HostState`, so an offline card keeps its logo.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use wado_protocol::relay_wire::normalize_remote_id;

use crate::store;

const FILE: &str = "profiles.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Profile {
    /// Stable id of this computer on this device: its normalised Remote ID.
    pub key: String,
    pub name: String,
    pub relay_url: String,
    pub remote_id: String,
    #[serde(default)]
    pub os_id: String,
    #[serde(default)]
    pub os_like: Vec<String>,
    #[serde(default)]
    pub desktop: String,
    /// Which base16 accent slot (0..8 → base08..base0F) its disc is drawn in.
    #[serde(default)]
    pub hue: u8,
    #[serde(default)]
    pub last_used_ms: u64,
}

impl Profile {
    /// A freshly paired computer, named by its Remote ID until its host state names it.
    pub fn new(relay_url: &str, remote_id: &str) -> Profile {
        let key = normalize_remote_id(remote_id);
        let hue = key
            .bytes()
            .fold(0u32, |h, b| h.wrapping_mul(31) + u32::from(b))
            % 8;
        Profile {
            name: wado_protocol::relay_wire::display_remote_id(&key),
            key,
            relay_url: relay_url.trim_end_matches('/').to_string(),
            remote_id: normalize_remote_id(remote_id),
            os_id: String::new(),
            os_like: Vec::new(),
            desktop: String::new(),
            hue: hue as u8,
            last_used_ms: 0,
        }
    }
}

pub struct Profiles {
    dir: PathBuf,
    list: Vec<Profile>,
}

impl Profiles {
    /// What is on file; an unreadable file reads as none (it is rewritten whole on the next save).
    pub fn load(dir: &Path) -> Profiles {
        let list = store::read(dir, FILE)
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        Profiles {
            dir: dir.to_path_buf(),
            list,
        }
    }

    pub fn list(&self) -> Vec<Profile> {
        self.list.clone()
    }

    pub fn get(&self, key: &str) -> Option<&Profile> {
        self.list.iter().find(|p| p.key == key)
    }

    /// Add or replace by key, in memory; [`Profiles::save`] writes it.
    pub fn upsert(&mut self, p: Profile) {
        match self.list.iter_mut().find(|q| q.key == p.key) {
            Some(q) => *q = p,
            None => self.list.push(p),
        }
    }

    pub fn forget(&mut self, key: &str) {
        self.list.retain(|p| p.key != key);
    }

    pub fn save(&self) -> std::io::Result<()> {
        let bytes = serde_json::to_vec_pretty(&self.list).expect("profiles serialise");
        store::write(&self.dir, FILE, &bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_is_atomic_and_roundtrips() {
        let d = store::temp_dir("profiles");
        let mut p = Profiles::load(&d);
        assert!(p.list().is_empty());
        let mut a = Profile::new("https://relay.example/", "528-491-307");
        a.os_id = "nixos".into();
        a.os_like = vec!["linux".into()];
        p.upsert(a.clone());
        p.upsert(Profile::new("https://relay.example", "111222333"));
        p.save().unwrap();
        assert!(!d.join("profiles.json.tmp").exists());
        let back = Profiles::load(&d);
        assert_eq!(back.list().len(), 2);
        assert_eq!(back.get("528491307"), Some(&a));
        assert_eq!(a.relay_url, "https://relay.example");
        assert_eq!(a.name, "528-491-307");
        let mut p = back;
        p.forget("528491307");
        p.save().unwrap();
        assert_eq!(Profiles::load(&d).list().len(), 1);
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn old_file_without_os_fields_loads() {
        let d = store::temp_dir("profiles-old");
        store::write(
            &d,
            FILE,
            br#"[{"key":"1","name":"box","relay_url":"https://r","remote_id":"1"}]"#,
        )
        .unwrap();
        let p = Profiles::load(&d);
        let one = p.get("1").unwrap();
        assert_eq!(one.os_id, "");
        assert!(one.os_like.is_empty());
        assert_eq!(one.hue, 0);
        let _ = std::fs::remove_dir_all(d);
    }
}
