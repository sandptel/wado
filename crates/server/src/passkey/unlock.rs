//! The unlock window: which device on this daemon has proven its passkey, and until when.
//!
//! It opens on a good assertion (or a fresh registration, which verified the person just the
//! same), stays open while that device keeps using it — every gated message and the app's
//! keepalive restart the idle clock — and shuts after [`IDLE`], on a lock from the device (its
//! app went to the background), or when a new connection takes this daemon's seat
//! ([`lock_all`]). A reconnect therefore asks again: a window belongs to the link that proved it,
//! not to whatever later claims the same device key.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Decided with the user (`2026-10-27`, again `2026-10-30`).
pub const IDLE: Duration = Duration::from_secs(15 * 60);

#[derive(Default)]
pub struct Window {
    open: HashMap<String, Instant>,
}

impl Window {
    pub fn open(&mut self, key: &str, now: Instant) {
        if !key.is_empty() {
            self.open.insert(key.to_string(), now);
        }
    }

    pub fn lock(&mut self, key: &str) {
        self.open.remove(key);
    }

    pub fn lock_all(&mut self) {
        self.open.clear();
    }

    /// Time left for `key`, without counting this as use.
    pub fn left(&mut self, key: &str, now: Instant) -> Option<Duration> {
        let at = *self.open.get(key)?;
        let gone = now.saturating_duration_since(at);
        if gone >= IDLE {
            self.open.remove(key);
            return None;
        }
        Some(IDLE - gone)
    }

    /// Is it open for `key`? If so, this use restarts the idle clock.
    pub fn touch(&mut self, key: &str, now: Instant) -> bool {
        let open = self.left(key, now).is_some();
        if open {
            self.open.insert(key.to_string(), now);
        }
        open
    }
}

fn global() -> std::sync::MutexGuard<'static, Window> {
    static W: std::sync::LazyLock<Mutex<Window>> = std::sync::LazyLock::new(Mutex::default);
    W.lock().unwrap_or_else(|e| e.into_inner())
}

pub fn open(key: &str) {
    global().open(key, Instant::now());
}

pub fn lock(key: &str) {
    global().lock(key);
}

/// A new device took the seat: nobody stays unlocked across it.
pub fn lock_all() {
    global().lock_all();
}

pub fn touch(key: &str) -> bool {
    global().touch(key, Instant::now())
}

pub fn left(key: &str) -> Option<Duration> {
    global().left(key, Instant::now())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fifteen_minutes_of_idle_then_locked() {
        let mut w = Window::default();
        let t = Instant::now();
        assert!(!w.touch("phone", t), "locked until proven");
        w.open("phone", t);
        assert!(w.touch("phone", t + Duration::from_secs(14 * 60)));
        // That use restarted the clock: 14 more minutes is still fine.
        assert!(w.touch("phone", t + Duration::from_secs(28 * 60)));
        assert!(
            !w.touch("phone", t + Duration::from_secs(28 * 60) + IDLE),
            "idle too long"
        );
        assert!(
            !w.touch("phone", t + Duration::from_secs(60 * 60)),
            "and stays locked"
        );
    }

    #[test]
    fn looking_is_not_using() {
        let mut w = Window::default();
        let t = Instant::now();
        w.open("phone", t);
        assert!(w.left("phone", t + Duration::from_secs(10 * 60)).is_some());
        assert!(
            w.left("phone", t + IDLE).is_none(),
            "a look ten minutes in did not extend it"
        );
    }

    #[test]
    fn only_the_device_that_proved_it_and_a_lock_shuts_it() {
        let mut w = Window::default();
        let t = Instant::now();
        w.open("phone", t);
        assert!(!w.touch("tablet", t));
        assert!(!w.touch("", t));
        w.open("", t);
        assert!(!w.touch("", t), "no key, never open");
        assert!(w.touch("phone", t));
        w.lock("phone");
        assert!(!w.touch("phone", t), "backgrounded: locked at once");
        w.open("phone", t);
        w.open("tablet", t);
        w.lock_all();
        assert!(
            !w.touch("phone", t) && !w.touch("tablet", t),
            "the seat changed hands"
        );
    }
}
