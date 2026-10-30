//! The unlock window: which device on this daemon has proven its passkey, and until when.
//!
//! It opens on a good assertion (or a fresh registration, which verified the person just the
//! same), stays open while that device keeps using it — every gated message and the app's
//! keepalive restart the idle clock — and shuts after its idle time, on a lock from the device
//! (Lock now, or its app went to the background), or from the computer (`wado passkey lock`).
//!
//! How long, and what it outlives, is the device's own choice ([`super::stay`]). By default
//! (15 minutes) a window belongs to the link that proved it: a new connection taking this
//! daemon's seat shuts it ([`lock_link`]), so a reconnect asks again. A device that chose longer
//! keeps its window across its own reconnects — the envelope proves the same device key, which
//! is what the window is bound to — and "until I lock it" on a code-pinned line also across a
//! daemon restart, read back from its file ([`restore`]).

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use super::Store;
use super::stay::How;

/// The default idle window. Decided with the user (`2026-10-27`, again `2026-10-30`).
pub const IDLE: Duration = Duration::from_secs(15 * 60);

/// What [`Window::left`] says of a window with no idle clock.
pub const FOREVER: Duration = Duration::MAX;

#[derive(Clone, Copy)]
struct Open {
    at: Instant,
    how: How,
    /// Opened by an approval on the computer for a device with no passkey here
    /// ([`open_approved`]): the only window a device without one can have.
    bare: bool,
}

#[derive(Default)]
pub struct Window {
    open: HashMap<String, Open>,
}

impl Window {
    pub fn open(&mut self, key: &str, now: Instant, how: How) {
        self.open_as(key, now, how, false);
    }

    fn open_as(&mut self, key: &str, now: Instant, how: How, bare: bool) {
        if !key.is_empty() {
            self.open
                .insert(key.to_string(), Open { at: now, how, bare });
        }
    }

    /// Open for a device that has no passkey here — approved on the computer.
    pub fn open_bare(&mut self, key: &str, now: Instant) {
        self.open_as(key, now, How::default(), true);
    }

    pub fn is_bare(&self, key: &str) -> bool {
        self.open.get(key).is_some_and(|o| o.bare)
    }

    /// A new choice for an open window; its idle clock restarts. False when it is not open.
    pub fn set(&mut self, key: &str, now: Instant, how: How) -> bool {
        if self.left(key, now).is_none() {
            return false;
        }
        self.open(key, now, how);
        true
    }

    pub fn lock(&mut self, key: &str) {
        self.open.remove(key);
    }

    pub fn lock_all(&mut self) {
        self.open.clear();
    }

    /// A link came or went: shut every window bound to its link, and any kept one `still` says
    /// no longer stands (its passkey removed, its device unpaired, locked from elsewhere).
    pub fn lock_link(&mut self, still: impl Fn(&str, &How) -> bool) {
        self.open.retain(|k, o| o.how.keep && still(k, &o.how));
    }

    pub fn is_open(&self, key: &str) -> bool {
        self.open.contains_key(key)
    }

    /// Time left for `key` ([`FOREVER`] without an idle clock), without counting this as use.
    pub fn left(&mut self, key: &str, now: Instant) -> Option<Duration> {
        let o = *self.open.get(key)?;
        let Some(idle) = o.how.idle else {
            return Some(FOREVER);
        };
        let gone = now.saturating_duration_since(o.at);
        if gone >= idle {
            self.open.remove(key);
            return None;
        }
        Some(idle - gone)
    }

    /// Is it open for `key`? If so, this use restarts the idle clock.
    pub fn touch(&mut self, key: &str, now: Instant) -> bool {
        let open = self.left(key, now).is_some();
        if open && let Some(o) = self.open.get_mut(key) {
            o.at = now;
        }
        open
    }
}

fn global() -> std::sync::MutexGuard<'static, Window> {
    static W: std::sync::LazyLock<Mutex<Window>> = std::sync::LazyLock::new(Mutex::default);
    W.lock().unwrap_or_else(|e| e.into_inner())
}

/// `key`'s choice, as it stands on file and on the trust list.
fn how_for(store: &Store, key: &str) -> How {
    let (proven, pinned) = store
        .gate()
        .entry(key)
        .map_or((false, false), |(pk, pinned)| (!pk.is_empty(), pinned));
    super::stay::how(&store.prefs(key), proven, pinned)
}

/// Mark or clear "open across a restart" on file, as `how` says.
fn persist(store: &Store, key: &str, how: How) {
    let mut p = store.prefs(key);
    let want = how.persist.then(super::now_s);
    if p.open_since.is_some() == want.is_some() {
        return;
    }
    p.open_since = want;
    if let Err(e) = store.set_prefs(key, &p) {
        tracing::warn!("passkey: could not save the unlock: {e}");
    }
}

/// Open `key`'s window, as its choice says — after a good assertion or a new passkey.
pub fn open(key: &str) {
    open_in(&Store::default(), key);
}

pub(crate) fn open_in(store: &Store, key: &str) {
    let how = how_for(store, key);
    global().open(key, Instant::now(), how);
    persist(store, key, how);
}

/// Someone at the computer approved `key` (`super::approve`): the same window a passkey would
/// open — its own lifetime rules — or, for a device with no passkey here, the default one,
/// bound to its link.
pub(crate) fn open_approved(store: &Store, key: &str) {
    if store.has(key) {
        open_in(store, key);
    } else {
        global().open_bare(key, Instant::now());
    }
}

/// Is `key` open by an approval though it has no passkey? A use restarts its idle clock.
pub(crate) fn bare_open(key: &str, use_it: bool) -> bool {
    let mut w = global();
    if !w.is_bare(key) {
        return false;
    }
    let now = Instant::now();
    if use_it {
        w.touch(key, now)
    } else {
        w.left(key, now).is_some()
    }
}

/// `key` changed its choice while unlocked: apply it to the open window. False if not open.
pub(crate) fn apply(store: &Store, key: &str) -> bool {
    let how = how_for(store, key);
    let open = global().set(key, Instant::now(), how);
    if open {
        persist(store, key, how);
    }
    open
}

/// Lock `key` now, here and on file.
pub fn lock(key: &str) {
    lock_in(&Store::default(), key);
}

pub(crate) fn lock_in(store: &Store, key: &str) {
    global().lock(key);
    let mut p = store.prefs(key);
    if p.open_since.take().is_some() {
        let _ = store.set_prefs(key, &p);
    }
}

/// Every device: the computer's "lock everything".
pub fn lock_every() {
    let store = Store::default();
    global().lock_all();
    for p in store.all_prefs() {
        lock_in(&store, &p.key);
    }
}

/// A link took this daemon's seat or left it: every window bound to a link shuts; a kept one
/// stays only while its passkey, its trust line and (for one held across restarts) its mark on
/// file still stand.
pub fn lock_link() {
    lock_link_in(&Store::default());
}

pub(crate) fn lock_link_in(store: &Store) {
    let gate = store.gate();
    global().lock_link(|key, how| {
        store.has(key)
            && gate.entry(key).is_some()
            && (!how.persist || store.prefs(key).open_since.is_some())
    });
}

/// After a restart (or on a sibling daemon of the pool): open `key` again if it chose "until I
/// lock it", was unlocked when last seen, and its line is still code-pinned. True if open now.
pub fn restore(key: &str) -> bool {
    restore_in(&Store::default(), key)
}

pub(crate) fn restore_in(store: &Store, key: &str) -> bool {
    if key.is_empty() {
        return false;
    }
    let open = global().is_open(key);
    if open {
        return true;
    }
    if store.prefs(key).open_since.is_none() || !store.has(key) {
        return false;
    }
    let how = how_for(store, key);
    if !how.persist {
        return false;
    }
    global().open(key, Instant::now(), how);
    true
}

// Each lock of the window is its own statement: `restore` takes it again.
pub fn touch(key: &str) -> bool {
    touch_in(&Store::default(), key)
}

pub(crate) fn touch_in(store: &Store, key: &str) -> bool {
    let open = global().touch(key, Instant::now());
    open || (restore_in(store, key) && global().touch(key, Instant::now()))
}

pub fn left(key: &str) -> Option<Duration> {
    left_in(&Store::default(), key)
}

pub(crate) fn left_in(store: &Store, key: &str) -> Option<Duration> {
    let left = global().left(key, Instant::now());
    left.or_else(|| restore_in(store, key).then_some(FOREVER))
}

/// The daemon's memory of `key`'s window gone, its files kept — a restart, for a test.
#[cfg(test)]
pub(crate) fn forget(key: &str) {
    global().lock(key);
}

#[cfg(test)]
mod tests {
    use super::*;

    const D: How = How {
        idle: Some(IDLE),
        keep: false,
        persist: false,
    };
    fn timed(h: u64) -> How {
        How {
            idle: Some(Duration::from_secs(h * 3600)),
            keep: true,
            persist: false,
        }
    }
    const UNTIL: How = How {
        idle: None,
        keep: true,
        persist: true,
    };

    #[test]
    fn fifteen_minutes_of_idle_then_locked() {
        let mut w = Window::default();
        let t = Instant::now();
        assert!(!w.touch("phone", t), "locked until proven");
        w.open("phone", t, D);
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
        w.open("phone", t, D);
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
        w.open("phone", t, D);
        assert!(!w.touch("tablet", t));
        assert!(!w.touch("", t));
        w.open("", t, D);
        assert!(!w.touch("", t), "no key, never open");
        assert!(w.touch("phone", t));
        w.lock("phone");
        assert!(!w.touch("phone", t), "backgrounded: locked at once");
        w.open("phone", t, D);
        w.open("tablet", t, D);
        w.lock_link(|_, _| true);
        assert!(
            !w.touch("phone", t) && !w.touch("tablet", t),
            "the seat changed hands"
        );
    }

    #[test]
    fn each_lifetime_idles_out_on_its_own_clock() {
        let t = Instant::now();
        for h in [1, 8] {
            let mut w = Window::default();
            w.open("phone", t, timed(h));
            let idle = Duration::from_secs(h * 3600);
            assert!(
                w.left("phone", t + IDLE + Duration::from_secs(1)).is_some(),
                "{h}h outlasts 15 min"
            );
            assert!(w.touch("phone", t + idle - Duration::from_secs(1)));
            assert!(
                w.left("phone", t + 2 * idle - Duration::from_secs(2))
                    .is_some(),
                "{h}h: use restarted it"
            );
            assert!(w.left("phone", t + 2 * idle).is_none(), "{h}h idle: locked");
        }
        let mut w = Window::default();
        w.open("phone", t, UNTIL);
        assert_eq!(
            w.left("phone", t + Duration::from_secs(400 * 24 * 3600)),
            Some(FOREVER),
            "until I lock it: no idle clock"
        );
        w.lock("phone");
        assert!(w.left("phone", t).is_none(), "and Lock now still locks it");
    }

    #[test]
    fn a_chosen_lifetime_survives_a_reconnect_the_default_does_not() {
        let t = Instant::now();
        let mut w = Window::default();
        w.open("default", t, D);
        w.open("hour", t, timed(1));
        w.open("until", t, UNTIL);
        // A new link on the seat (the same phone reconnecting, or another device).
        w.lock_link(|_, _| true);
        assert!(w.left("default", t).is_none(), "15 min: bound to its link");
        assert!(w.left("hour", t).is_some(), "1 hour: kept for its device");
        assert!(w.left("until", t).is_some(), "until I lock it: kept");
        // A kept one that no longer stands (passkey removed, unpaired, locked elsewhere).
        w.lock_link(|k, _| k != "until");
        assert!(w.left("until", t).is_none() && w.left("hour", t).is_some());
    }

    #[test]
    fn a_new_choice_applies_only_while_open() {
        let t = Instant::now();
        let mut w = Window::default();
        assert!(!w.set("phone", t, UNTIL), "locked: nothing to change");
        assert!(w.left("phone", t).is_none());
        w.open("phone", t, D);
        assert!(w.set("phone", t + Duration::from_secs(60), UNTIL));
        assert_eq!(w.left("phone", t + IDLE * 10), Some(FOREVER));
    }
}
