//! Each device's push subscription and the kinds it wants: `<config>/wado/push/<sha256 of the
//! device key>`, JSON, this user only. Shared by the pool, like the trust list.

use std::path::PathBuf;

use base64::Engine as _;
use serde::{Deserialize, Serialize};
use wado_protocol::push::{PushKinds, PushSub, SUB_UNIFIEDPUSH, SUB_WEB};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    /// The device key it belongs to.
    pub device: String,
    pub sub: PushSub,
    pub kinds: PushKinds,
    #[serde(default)]
    pub added: u64,
}

#[derive(Clone)]
pub struct Subs {
    dir: PathBuf,
}

impl Default for Subs {
    fn default() -> Self {
        Self::at(crate::remote_id::config_dir().join("push"))
    }
}

fn file_name(key: &str) -> String {
    ring::digest::digest(&ring::digest::SHA256, key.as_bytes())
        .as_ref()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn d(s: &str) -> Option<Vec<u8>> {
    base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(s.trim_end_matches('='))
        .ok()
}

/// Is `sub` something this computer will send to? The reason when not.
pub fn check(sub: &PushSub) -> Result<(), String> {
    if sub.endpoint.len() > 2048 {
        return Err("that push address is too long".into());
    }
    super::vapid::audience(&sub.endpoint)?;
    if d(&sub.p256dh).is_none_or(|k| k.len() != 65 || k[0] != 4) {
        return Err("that subscription's key is not a P-256 key".into());
    }
    if d(&sub.auth).is_none_or(|a| a.len() != 16) {
        return Err("that subscription's secret is not 16 bytes".into());
    }
    if !sub.kind.is_empty() && sub.kind != SUB_WEB && sub.kind != SUB_UNIFIEDPUSH {
        return Err(format!("`{}` is not a kind of push subscription", sub.kind));
    }
    Ok(())
}

impl Subs {
    pub fn at(dir: PathBuf) -> Self {
        Self { dir }
    }

    pub fn get(&self, key: &str) -> Option<Entry> {
        let t = std::fs::read_to_string(self.dir.join(file_name(key))).ok()?;
        serde_json::from_str(&t).ok()
    }

    pub fn put(&self, e: &Entry) -> std::io::Result<()> {
        use std::io::Write as _;
        use std::os::unix::fs::OpenOptionsExt as _;
        std::fs::create_dir_all(&self.dir)?;
        let name = file_name(&e.device);
        let tmp = self.dir.join(format!(".{name}.tmp"));
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .mode(0o600)
            .open(&tmp)?;
        f.write_all(serde_json::to_string_pretty(e)?.as_bytes())?;
        std::fs::rename(tmp, self.dir.join(name))
    }

    pub fn remove(&self, key: &str) -> bool {
        std::fs::remove_file(self.dir.join(file_name(key))).is_ok()
    }

    /// Every subscription on file.
    pub fn all(&self) -> Vec<Entry> {
        std::fs::read_dir(&self.dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|f| !f.file_name().to_string_lossy().starts_with('.'))
            .filter_map(|f| std::fs::read_to_string(f.path()).ok())
            .filter_map(|t| serde_json::from_str(&t).ok())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sub() -> PushSub {
        PushSub {
            endpoint: "https://push.example/abc".into(),
            p256dh: base64::engine::general_purpose::URL_SAFE_NO_PAD.encode([4u8; 65]),
            auth: base64::engine::general_purpose::URL_SAFE_NO_PAD.encode([1u8; 16]),
            kind: String::new(),
        }
    }

    #[test]
    fn checks_what_it_will_send_to() {
        assert!(check(&sub()).is_ok());
        assert!(
            check(&PushSub {
                endpoint: "http://x/y".into(),
                ..sub()
            })
            .is_err()
        );
        assert!(
            check(&PushSub {
                p256dh: "AAAA".into(),
                ..sub()
            })
            .is_err()
        );
        assert!(
            check(&PushSub {
                auth: "AAAA".into(),
                ..sub()
            })
            .is_err()
        );
        assert!(
            check(&PushSub {
                kind: SUB_UNIFIEDPUSH.into(),
                ..sub()
            })
            .is_ok()
        );
        assert!(
            check(&PushSub {
                kind: "carrier-pigeon".into(),
                ..sub()
            })
            .is_err()
        );
    }

    #[test]
    fn per_device_on_file() {
        let dir = std::env::temp_dir().join(format!("wado-push-{}", rand::random::<u64>()));
        let s = Subs::at(dir.clone());
        let e = Entry {
            device: "phone".into(),
            sub: sub(),
            kinds: PushKinds::default(),
            added: 1,
        };
        s.put(&e).unwrap();
        assert_eq!(s.get("phone"), Some(e.clone()));
        assert_eq!(s.all(), vec![e]);
        assert!(s.remove("phone") && s.get("phone").is_none());
        let _ = std::fs::remove_dir_all(dir);
    }
}
