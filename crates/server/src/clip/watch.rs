//! Noticing the host clipboard change, through the platform's clipboard
//! (`wado_platform::clip::Clipboard::changes`). Every viewer and the session sync subscribe.

use tokio::sync::watch;

/// Ticks once per host clipboard change; never ticks without a desktop clipboard.
pub fn changes() -> watch::Receiver<u64> {
    match crate::platform::get().clipboard() {
        Ok(c) => c.changes(),
        Err(_) => watch::channel(0).1,
    }
}

/// Start watching for the life of the daemon. Nothing to watch outside a Wayland desktop.
pub fn start() {
    let _ = changes();
}
