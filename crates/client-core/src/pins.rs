//! Each computer's identity, pinned per Remote ID: from the QR link's `hk`, else on first sight.
//! A computer that later presents another key is refused outright — that is what a relay standing
//! in for it looks like. The JS twin is `js/e2e_keys.js` (`wado.hostpins`).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use wado_protocol::relay_wire::normalize_remote_id;

use crate::store;

const FILE: &str = "pins.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PinFrom {
    #[serde(rename = "qr")]
    Qr,
    #[serde(rename = "first-use")]
    FirstUse,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pin {
    /// base64url(SHA-256(host public key)), as `wado qr` puts in `hk`.
    pub pin: String,
    pub from: PinFrom,
}

pub struct Pins {
    dir: PathBuf,
    all: BTreeMap<String, Pin>,
}

impl Pins {
    pub fn load(dir: &Path) -> Pins {
        let all = store::read(dir, FILE)
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        Pins {
            dir: dir.to_path_buf(),
            all,
        }
    }

    pub fn get(&self, remote_id: &str) -> Option<Pin> {
        self.all.get(&normalize_remote_id(remote_id)).cloned()
    }

    /// Pin `remote_id` and save at once.
    pub fn set(&mut self, remote_id: &str, pin: String, from: PinFrom) -> std::io::Result<()> {
        self.all
            .insert(normalize_remote_id(remote_id), Pin { pin, from });
        let bytes = serde_json::to_vec(&self.all).expect("pins serialise");
        store::write(&self.dir, FILE, &bytes)
    }

    pub fn forget(&mut self, remote_id: &str) -> std::io::Result<()> {
        self.all.remove(&normalize_remote_id(remote_id));
        let bytes = serde_json::to_vec(&self.all).expect("pins serialise");
        store::write(&self.dir, FILE, &bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pins_persist_by_normalised_id_and_wire_names_match_js() {
        let d = store::temp_dir("pins");
        let mut p = Pins::load(&d);
        p.set("528-491-307", "abc".into(), PinFrom::Qr).unwrap();
        let again = Pins::load(&d);
        assert_eq!(
            again.get("528491307"),
            Some(Pin {
                pin: "abc".into(),
                from: PinFrom::Qr
            })
        );
        let raw = String::from_utf8(store::read(&d, FILE).unwrap()).unwrap();
        assert!(raw.contains(r#""from":"qr""#), "{raw}");
        p.set("1", "x".into(), PinFrom::FirstUse).unwrap();
        let raw = String::from_utf8(store::read(&d, FILE).unwrap()).unwrap();
        assert!(raw.contains(r#""from":"first-use""#), "{raw}");
        let _ = std::fs::remove_dir_all(d);
    }
}
