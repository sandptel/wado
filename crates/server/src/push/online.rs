//! "Back online": told when a daemon reaches the relay again — the first time after the
//! computer starts, and after it was away from the relay longer than [`AWAY`]. Once per pool,
//! not per daemon (`super::once`). The going-offline half cannot be sent from here; see the
//! module docs of [`super`].

use std::sync::Mutex;
use std::time::{Duration, Instant};

use wado_protocol::push::{NOTE_ONLINE, PushNote};

/// Shorter than this away is a blip (a relay restart, a network change) — not news.
pub const AWAY: Duration = Duration::from_secs(120);

static LOST: Mutex<Option<Instant>> = Mutex::new(None);
static FIRST: Mutex<bool> = Mutex::new(true);

/// Held while registered with the relay; dropping it marks the moment the link was lost.
pub struct Up;

impl Drop for Up {
    fn drop(&mut self) {
        *LOST.lock().unwrap_or_else(|e| e.into_inner()) = Some(Instant::now());
    }
}

/// What to say now that the link is back, if anything.
pub fn verdict(first: bool, away: Option<Duration>) -> Option<&'static str> {
    if first {
        Some("is online")
    } else if away.is_some_and(|a| a >= AWAY) {
        Some("is back online")
    } else {
        None
    }
}

fn boot_id() -> String {
    std::fs::read_to_string("/proc/sys/kernel/random/boot_id")
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

/// Registered with the relay. Keep the returned guard for as long as the link lasts.
pub fn registered() -> Up {
    let first = std::mem::replace(&mut *FIRST.lock().unwrap_or_else(|e| e.into_inner()), false);
    let away = LOST
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .take()
        .map(|t| t.elapsed());
    // The devices were told the *relay* was gone (`crate::relay_watch`): the computer never was,
    // and "relay is back" is that module's to say.
    let away = away.filter(|_| !crate::relay_watch::state::told());
    if let Some(words) = verdict(first, away) {
        // Once per machine boot for the first; once per minute for a return.
        let tag = if first {
            format!("online-{}", boot_id())
        } else {
            format!("online-{}", crate::passkey::now_s() / 60)
        };
        if super::once(&tag) {
            let (computer, name) = super::computer();
            super::deliver(
                PushNote {
                    kind: NOTE_ONLINE.into(),
                    title: format!("{name} {words}"),
                    body: "Tap to open it.".into(),
                    computer,
                    computer_name: name,
                    tag: "online".into(),
                    at: crate::passkey::now_s(),
                    ..Default::default()
                },
                |e| e.kinds.online,
            );
        }
    }
    Up
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn says_it_once_at_start_and_after_a_real_absence() {
        assert_eq!(verdict(true, None), Some("is online"));
        assert_eq!(verdict(false, Some(Duration::from_secs(5))), None, "a blip");
        assert_eq!(verdict(false, Some(AWAY)), Some("is back online"));
        assert_eq!(verdict(false, None), None);
    }
}
