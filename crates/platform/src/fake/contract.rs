//! What any provider of a capability must do — checked against the fakes here, and against each
//! real provider in its own (live) tests. A failed check panics with what was broken.

use std::time::{Duration, Instant};

use wado_protocol::pad::Mod;

use crate::audio::AudioMixer;
use crate::display::Brightness;
use crate::input::{KeyInjector, Opener, TouchInjector};
use crate::media::MediaPlayers;
use crate::network::{Bluetooth, WifiRadio};
use crate::{Capability, Fut, Platform};

/// A capability that publishes state: what [`stateful`] checks.
pub trait Fresh: Sync {
    fn fresh(&self, within: Duration) -> Fut<'_, ()>;
    /// A subscriber to the published state is still connected.
    fn subscribed(&self) -> bool;
}

macro_rules! fresh {
    ($($t:ty),*) => {$(
        impl Fresh for $t {
            fn fresh(&self, within: Duration) -> Fut<'_, ()> {
                <Self>::fresh(self, within)
            }
            fn subscribed(&self) -> bool {
                self.state().has_changed().is_ok()
            }
        }
    )*};
}
fresh!(
    dyn AudioMixer,
    dyn WifiRadio,
    dyn Bluetooth,
    dyn Brightness,
    dyn MediaPlayers
);

/// How long one fetch may take — the slowest tool's deadline, with room.
const FETCH: Duration = Duration::from_secs(10);
/// How long answering from what was already fetched may take.
const CACHED: Duration = Duration::from_millis(50);

/// Published state: a fetch finishes in bounded time, the watch stays connected, and an ask
/// right after a fetch is answered from it, not by another.
pub async fn stateful<C: Fresh + ?Sized>(c: &C) {
    let t = Instant::now();
    c.fresh(Duration::ZERO).await;
    assert!(t.elapsed() < FETCH, "a fetch took {:?}", t.elapsed());
    assert!(c.subscribed(), "the published state lost its sender");
    let t = Instant::now();
    c.fresh(Duration::from_secs(3600)).await;
    assert!(
        t.elapsed() < CACHED,
        "an ask within the age fetched again ({:?})",
        t.elapsed()
    );
}

/// A touchpad: sized, driven, lifted, resized, dropped — without an error.
pub fn touch(open: &dyn Opener<dyn TouchInjector>) {
    let mut t = open.open().expect("open");
    t.resize(390, 800).expect("resize");
    t.frame(&[(0, 100, 100), (1, 30000, 30000)]).expect("frame");
    t.frame(&[(1, 30010, 30010)]).expect("one lifts");
    t.lift_all().expect("lift");
    t.resize(390, 800).expect("the same size again");
    t.resize(800, 390).expect("a new size");
    t.frame(&[(0, 65535, 65535)]).expect("the far corner");
    t.lift_all().expect("lift");
}

/// A keyboard: taps, holds, releases — and releasing twice is harmless.
pub fn keys(open: &dyn Opener<dyn KeyInjector>) {
    let mut k = open.open().expect("open");
    k.tap(&[], 30).expect("tap");
    k.tap(&[Mod::Shift], 30).expect("chord");
    k.hold(&[Mod::Ctrl]).expect("hold");
    k.tap(&[Mod::Ctrl], 46).expect("a tap under a held mod");
    k.release_all().expect("release");
    k.release_all().expect("release again");
}

/// The registry: every capability is either provided or absent with a reason.
pub fn platform(p: &Platform) {
    let caps = p.caps();
    assert_eq!(caps.on.len() + caps.off.len(), Capability::ALL.len());
    for off in &caps.off {
        assert!(
            !off.why.trim().is_empty(),
            "{} is absent with no reason",
            off.cap
        );
    }
}
