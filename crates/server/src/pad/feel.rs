//! A phone's touchpad feel (`device "<key>" { trackpad { speed … } }`), applied on the computer to
//! wado's touchpad device: when its pad opens, and again when the phone changes a setting while
//! the pad is open. The applying itself is the compositor's (`crate::platform::linux::hypr_device`);
//! this only says *when*, and always off the hot path (spawned).

use std::collections::BTreeSet;
use std::sync::Mutex;

/// The phones whose pad is open now, by device key.
static OPEN: Mutex<BTreeSet<String>> = Mutex::new(BTreeSet::new());

fn open() -> std::sync::MutexGuard<'static, BTreeSet<String>> {
    OPEN.lock().unwrap_or_else(|e| e.into_inner())
}

/// `key`'s pad opened (its touchpad device is being made): apply its settings.
pub fn opened(key: &str) {
    open().insert(key.to_string());
    apply(key);
}

/// `key`'s pad closed: its later changes wait for the next open.
pub fn closed(key: &str) {
    open().remove(key);
}

/// The config was written by `key` at `cfg_key`: re-apply when it is its own touchpad's feel and
/// its pad is open.
pub fn changed(key: &str, cfg_key: &str) {
    if mine(key, cfg_key) && open().contains(key) {
        apply(key);
    }
}

/// `cfg_key` is `key`'s own touchpad block.
fn mine(key: &str, cfg_key: &str) -> bool {
    !key.is_empty() && cfg_key.starts_with(&format!("device.{key}.trackpad."))
}

/// Apply `key`'s settings as the config has them now (the defaults when it has none).
fn apply(key: &str) {
    let t = wado_config::live::current()
        .device
        .get(key)
        .and_then(|d| d.trackpad.clone())
        .unwrap_or_default();
    if let Ok(rt) = tokio::runtime::Handle::try_current() {
        rt.spawn(crate::platform::linux::hypr_device::apply(t));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_its_own_touchpad_block_re_applies() {
        assert!(mine("k1", "device.k1.trackpad.speed"));
        assert!(mine("k1", "device.k1.trackpad.drag-lock"));
        assert!(!mine("k1", "device.k1.gamepad.layout"));
        assert!(!mine("k1", "device.k2.trackpad.speed"));
        assert!(!mine("k1", "device.k1x.trackpad.speed"));
        assert!(!mine("", "device..trackpad.speed"));
    }
}
