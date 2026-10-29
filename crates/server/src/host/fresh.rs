//! How old each part of the computer's state may be when a viewer asks for it — and making it
//! so, on demand, without letting one slow tool hold up the answer.
//!
//! The capabilities publish their state (`wado_platform::state`); a viewer's poll reads it. Each
//! poll only refreshes what is older than its age here, so however many viewers poll, each tool
//! runs at most once per age. A change a viewer just made marks its capability, and the next
//! answer is read after that change has had [`LAND`] to take effect — only that capability is
//! read again, not all of them.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

use tokio::time::Instant;
use wado_platform::{Capability, Platform};

/// The age each refreshed capability may have. Sound and players change under the person's
/// fingers and are cheap; Wi-Fi and Bluetooth are several processes each and change slowly.
pub fn age(cap: Capability) -> Duration {
    match cap {
        Capability::Audio | Capability::Media | Capability::Brightness => Duration::from_secs(1),
        Capability::Wifi | Capability::Bluetooth => Duration::from_secs(5),
        _ => Duration::from_secs(1),
    }
}

/// A moment for a change to land before it is read back.
pub const LAND: Duration = Duration::from_millis(250);

/// The longest an answer waits for refreshes: past it the viewer gets what is published, and
/// the refresh carries on for the next answer. A hung tool costs one stale value, not a frozen
/// control centre.
pub const WAIT: Duration = Duration::from_secs(2);

/// When each capability was last changed by a viewer.
#[derive(Default)]
pub struct Changed(Mutex<HashMap<Capability, Instant>>);

impl Changed {
    /// `cap` was just changed.
    pub fn mark(&self, cap: Capability) {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(cap, Instant::now());
    }

    /// How old `cap`'s state may be now: its age, or less — a read begun after its last change
    /// had landed.
    pub fn within(&self, cap: Capability) -> Duration {
        let changed = self
            .0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&cap)
            .copied();
        let since_landed = changed.map(|t| Instant::now().saturating_duration_since(t + LAND));
        since_landed.map_or(age(cap), |d| d.min(age(cap)))
    }
}

/// Make every refreshed capability no older than it may be, waiting at most [`WAIT`].
pub async fn refresh(p: &'static Platform, changed: &Changed) {
    let mut tasks = Vec::new();
    let audio = p.audio().ok();
    if let Some(a) = audio.clone() {
        let w = changed.within(Capability::Audio);
        tasks.push(tokio::spawn(async move { a.fresh(w).await }));
    }
    if let Ok(m) = p.media() {
        // Players are tied to the mixer's streams: the mixer is read first (one read either way —
        // concurrent asks share it).
        let (wa, wm) = (
            changed.within(Capability::Audio),
            changed.within(Capability::Media),
        );
        tasks.push(tokio::spawn(async move {
            if let Some(a) = audio {
                a.fresh(wa).await;
            }
            m.fresh(wm).await
        }));
    }
    if let Ok(b) = p.brightness() {
        let w = changed.within(Capability::Brightness);
        tasks.push(tokio::spawn(async move { b.fresh(w).await }));
    }
    if let Ok(wifi) = p.wifi() {
        let w = changed.within(Capability::Wifi);
        tasks.push(tokio::spawn(async move { wifi.fresh(w).await }));
    }
    if let Ok(bt) = p.bluetooth() {
        let w = changed.within(Capability::Bluetooth);
        tasks.push(tokio::spawn(async move { bt.fresh(w).await }));
    }
    let _ = tokio::time::timeout(WAIT, futures_util::future::join_all(tasks)).await;
}
