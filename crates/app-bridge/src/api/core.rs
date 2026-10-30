//! The core as Dart sees it. Commands return at once; everything the core reports arrives on the
//! one event stream (`events`) as small JSON objects (`event.rs`), so the Dart side reads them
//! the way the web client reads its messages.

use std::path::PathBuf;
use std::sync::Arc;

use flutter_rust_bridge::frb;
use wado_client_core::core::Core;
use wado_client_core::link::state::Room;

use super::event;
use super::secret::AesBox;
use crate::frb_generated::StreamSink;

#[frb(opaque)]
pub struct AppCore {
    core: Arc<Core>,
    key_reset: bool,
    _lock: std::fs::File,
}

impl AppCore {
    /// Start the core over `dir` with the keystore's 32-byte `box_key`. A box key that no longer
    /// opens this device's identity (the keystore was wiped) starts a new device: the old file is
    /// kept as `identity.lost` and `key_reset()` says so. A second core on the same `dir` is
    /// refused while the first runs.
    pub fn start(dir: String, box_key: Vec<u8>, device_name: String) -> anyhow::Result<AppCore> {
        let dir = PathBuf::from(dir);
        std::fs::create_dir_all(&dir)?;
        let lock = std::fs::File::create(dir.join("core.lock"))?;
        if lock.try_lock().is_err() {
            anyhow::bail!("wado is already running");
        }
        let sb = Arc::new(AesBox::new(&box_key).map_err(anyhow::Error::msg)?);
        let (core, key_reset) = match Core::new(dir.clone(), sb.clone(), device_name.clone()) {
            Ok(c) => (c, false),
            Err(_) if dir.join("identity.bin").exists() => {
                std::fs::rename(dir.join("identity.bin"), dir.join("identity.lost"))?;
                let c = Core::new(dir, sb, device_name).map_err(anyhow::Error::msg)?;
                (c, true)
            }
            Err(e) => anyhow::bail!(e),
        };
        Ok(AppCore {
            core: Arc::new(core),
            key_reset,
            _lock: lock,
        })
    }

    #[frb(sync)]
    pub fn key_reset(&self) -> bool {
        self.key_reset
    }

    #[frb(sync)]
    pub fn client_key(&self) -> String {
        self.core.client_key()
    }

    /// The saved computers, as a JSON array (the shape of the `computers` event's `list`).
    #[frb(sync)]
    pub fn computers(&self) -> String {
        event::computers_json(&self.core.computers())
    }

    #[frb(sync)]
    pub fn pair(&self, link: String) -> u64 {
        self.core.pair(link)
    }

    #[frb(sync)]
    pub fn pair_typed(&self, relay: String, remote_id: String, code: String) -> u64 {
        self.core.pair_typed(relay, remote_id, code)
    }

    #[frb(sync)]
    pub fn forget(&self, key: String) -> u64 {
        self.core.forget(key)
    }

    /// Open a room on computer `key`: `kind` is "seat", "control" or "view" (with `instance`).
    /// `takeover` asks for the seat on the first dial only. Returns the room id, or "" when this
    /// device does not know the computer.
    #[frb(sync)]
    pub fn open_room(&self, key: String, kind: String, instance: String, takeover: bool) -> String {
        let room = match kind.as_str() {
            "seat" => Room::Seat { takeover },
            "view" => Room::View { instance },
            _ => Room::Control,
        };
        self.core.open_room(key, room).unwrap_or_default()
    }

    /// The computer's host state as a room read it (`host_state.state`, JSON).
    #[frb(sync)]
    pub fn note_host(&self, key: String, state_json: String) {
        self.core.note_host(&key, &state_json)
    }

    #[frb(sync)]
    pub fn send_room(&self, id: String, json: String, urgent: bool, resend: bool) -> bool {
        self.core.send_room(&id, json, urgent, resend)
    }

    #[frb(sync)]
    pub fn close_room(&self, id: String) {
        self.core.close_room(&id)
    }

    #[frb(sync)]
    pub fn set_keepalive(&self, silence_ms: u64) {
        self.core.set_keepalive(silence_ms)
    }

    /// Every event, in order, as JSON (`event.rs`). Call once.
    pub fn events(&self, sink: StreamSink<String>) {
        let core = self.core.clone();
        std::thread::Builder::new()
            .name("wado-events".into())
            .spawn(move || {
                let rt = crate::exec::Executor;
                loop {
                    let e = rt.block_on(core.next_event());
                    if sink.add(event::json(&e)).is_err() {
                        return;
                    }
                }
            })
            .expect("event thread");
    }

    pub fn shutdown(&self) {
        self.core.shutdown()
    }
}

/// The OS logo slugs the core can name, for the app's asset check.
#[frb(sync)]
pub fn os_logo_slugs() -> Vec<String> {
    wado_client_core::os_logo::slugs()
        .iter()
        .map(|s| s.to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(n: &str) -> String {
        let d = std::env::temp_dir().join(format!("wado-app-bridge-{n}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d.to_string_lossy().into_owned()
    }

    #[test]
    fn second_core_on_same_dir_refuses() {
        let d = dir("lock");
        let a = AppCore::start(d.clone(), vec![1; 32], "p".into()).unwrap();
        let e = AppCore::start(d, vec![1; 32], "p".into()).err().unwrap();
        assert!(e.to_string().contains("already running"));
        assert!(!a.client_key().is_empty());
    }

    #[test]
    fn lost_box_key_starts_fresh_and_says_so() {
        let d = dir("lost");
        let k1 = {
            let a = AppCore::start(d.clone(), vec![1; 32], "p".into()).unwrap();
            assert!(!a.key_reset());
            a.client_key()
        };
        let b = AppCore::start(d.clone(), vec![2; 32], "p".into()).unwrap();
        assert!(b.key_reset());
        assert_ne!(b.client_key(), k1);
        assert!(std::path::Path::new(&d).join("identity.lost").exists());
        let k2 = b.client_key();
        drop(b);
        let c = AppCore::start(d, vec![2; 32], "p".into()).unwrap();
        assert!(!c.key_reset());
        assert_eq!(c.client_key(), k2);
    }
}
