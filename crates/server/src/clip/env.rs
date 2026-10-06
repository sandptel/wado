//! The desktop's `WAYLAND_DISPLAY`, read once before `wado_compositor::build` overwrites it with
//! the session's own socket for the apps it launches.

use std::sync::OnceLock;

static HOST: OnceLock<Option<String>> = OnceLock::new();

/// Call before the compositor is built. Later calls change nothing.
pub fn capture() {
    HOST.get_or_init(|| {
        std::env::var("WAYLAND_DISPLAY")
            .ok()
            .filter(|s| !s.is_empty())
    });
}

/// `None`: the daemon was not started inside a Wayland desktop (a tty, ssh, a service).
pub fn display() -> Option<&'static str> {
    HOST.get().and_then(|d| d.as_deref())
}
