//! The computer itself, controlled from the viewer: its sound, Wi-Fi, Bluetooth, sleep.
//!
//! Works with no session running — only the relay link is needed — so a phone with just a
//! shell open can still turn the computer's music down or move it to the phone.
//!
//! The mechanisms are the platform's (`wado_platform`, provided on Linux by
//! `crate::platform::linux`); this is the policy over them, written once:
//!
//! - [`fresh`] — how old each part of the state may be when a viewer asks, and refreshing it.
//! - [`audio`] — which outputs a viewer is shown; everything to one output.
//! - [`viewer`] — "This phone": the viewer sink's lifecycle.
//! - [`wifi`] — Wi-Fi changes on probation.
//! - [`trim`] — leaving out pictures the viewer already has.

pub mod audio;
pub mod fresh;
pub mod trim;
pub mod viewer;
pub mod wifi;

use std::time::Duration;

use wado_platform::{Capability, Platform};
use wado_protocol::{HostAction, HostState};

pub use viewer::{phone, start, viewer};

/// What viewers changed lately, for the next answer to read back.
static CHANGED: std::sync::LazyLock<fresh::Changed> = std::sync::LazyLock::new(Default::default);

/// The computer's state for a viewer.
pub async fn state() -> HostState {
    state_of(crate::platform::get(), &CHANGED).await
}

/// The state from `p`: what is older than it may be is read again first (see [`fresh`]); the
/// rest is what the capabilities last published.
pub async fn state_of(p: &'static Platform, changed: &fresh::Changed) -> HostState {
    fresh::refresh(p, changed).await;
    let mut s = p.host_state();
    audio::decorate(&mut s.audio, viewer::name(p).as_deref());
    s.wifi_revert_in = wifi::revert_in();
    s
}

/// Do `action` to the computer.
pub async fn act(action: HostAction) -> Result<(), String> {
    act_on(crate::platform::get(), &CHANGED, action).await
}

/// The capability `action` changes.
fn touches(action: &HostAction) -> Capability {
    match action {
        HostAction::SinkVolume { .. }
        | HostAction::SinkMute { .. }
        | HostAction::DefaultSink { .. }
        | HostAction::StreamVolume { .. }
        | HostAction::StreamMute { .. }
        | HostAction::StreamTo { .. }
        | HostAction::AllTo { .. } => Capability::Audio,
        HostAction::WifiRadio { .. } | HostAction::WifiConnect { .. } | HostAction::WifiKeep => {
            Capability::Wifi
        }
        HostAction::BtPower { .. } | HostAction::BtConnect { .. } => Capability::Bluetooth,
        HostAction::KeepAwake { .. } => Capability::PowerInhibit,
        HostAction::Brightness { .. } => Capability::Brightness,
        HostAction::Media { .. } => Capability::Media,
    }
}

/// Only an address a Bluetooth stack could have listed can be named.
fn bt_address(addr: &str) -> Result<(), String> {
    if addr.len() == 17 && addr.chars().all(|c| c.is_ascii_hexdigit() || c == ':') {
        Ok(())
    } else {
        Err("not a Bluetooth address".into())
    }
}

/// Do `action` through `p`, and mark what it changed so the next answer reads it back.
pub async fn act_on(
    p: &Platform,
    changed: &fresh::Changed,
    action: HostAction,
) -> Result<(), String> {
    let cap = touches(&action);
    let done = run(p, action).await;
    changed.mark(cap);
    done
}

async fn run(p: &Platform, action: HostAction) -> Result<(), String> {
    let absent = |a: wado_platform::Absent| a.why;
    match action {
        HostAction::SinkVolume { id, volume } | HostAction::StreamVolume { id, volume } => {
            p.audio().map_err(absent)?.set_volume(id, volume).await
        }
        HostAction::SinkMute { id, muted } | HostAction::StreamMute { id, muted } => {
            p.audio().map_err(absent)?.set_mute(id, muted).await
        }
        HostAction::DefaultSink { name } => {
            let mixer = p.audio().map_err(absent)?;
            let a = audio::now(mixer.as_ref(), viewer::name(p).as_deref()).await;
            if !a.sinks.iter().any(|s| s.name == name) {
                return Err("no such output".into());
            }
            mixer.set_default(name).await
        }
        HostAction::StreamTo { id, sink } => p.audio().map_err(absent)?.move_stream(id, sink).await,
        HostAction::AllTo { sink } => {
            let mixer = p.audio().map_err(absent)?;
            audio::all_to(mixer.as_ref(), sink, viewer::name(p).as_deref()).await
        }
        HostAction::WifiRadio { on, minutes } => {
            wifi::radio(p.wifi().map_err(absent)?, on, minutes).await
        }
        HostAction::WifiConnect { ssid, password } => {
            wifi::connect(p.wifi().map_err(absent)?, ssid, password).await
        }
        HostAction::WifiKeep => {
            wifi::keep();
            Ok(())
        }
        HostAction::BtPower { on } => p.bluetooth().map_err(absent)?.power(on).await,
        HostAction::BtConnect { addr, connect } => {
            bt_address(&addr)?;
            p.bluetooth().map_err(absent)?.connect(addr, connect).await
        }
        HostAction::KeepAwake { on } => p.power_inhibit().map_err(absent)?.set(on),
        HostAction::Brightness { level } => p.brightness().map_err(absent)?.set(level).await,
        HostAction::Media { bus, op } => p.media().map_err(absent)?.act(bus, op).await,
    }
}

/// How long the relay waits after an action before reading the state back.
pub const SETTLE: Duration = fresh::LAND;

#[cfg(test)]
mod tests {
    use super::*;
    use wado_platform::fake::Fakes;

    fn platform(f: &Fakes) -> &'static Platform {
        Box::leak(Box::new(f.platform()))
    }

    /// `n` polls of the state, `every` apart.
    async fn polls(p: &'static Platform, changed: &fresh::Changed, n: u32, every: Duration) {
        for _ in 0..n {
            state_of(p, changed).await;
            tokio::time::sleep(every).await;
        }
    }

    fn fetches(f: &Fakes) -> [u64; 5] {
        [
            f.audio.state.fetches(),
            f.media.state.fetches(),
            f.brightness.state.fetches(),
            f.wifi.state.fetches(),
            f.bluetooth.state.fetches(),
        ]
    }

    /// The measurement behind the refresh ages: a minute of the control centre open while
    /// something plays (the client polls every 1.2 s) — before, every poll ran every tool.
    #[tokio::test(start_paused = true)]
    async fn a_minute_of_polling_reads_wifi_and_bluetooth_a_fifth_as_often() {
        let f = Fakes::new();
        let p = platform(&f);
        let changed = fresh::Changed::default();
        polls(p, &changed, 50, Duration::from_millis(1200)).await;
        // [audio, media, brightness, wifi, bluetooth]; before: [50; 5].
        assert_eq!(fetches(&f), [50, 50, 50, 10, 10]);
    }

    /// Two viewers polling at once — the home page's link (3 s) and the control centre (1.2 s) —
    /// share one read instead of each running every tool.
    #[tokio::test(start_paused = true)]
    async fn two_pollers_share_the_reads() {
        let f = Fakes::new();
        let p = platform(&f);
        let changed = fresh::Changed::default();
        let fast = polls(p, &changed, 50, Duration::from_millis(1200));
        let slow = polls(p, &changed, 20, Duration::from_millis(3000));
        tokio::join!(fast, slow);
        // 70 polls in a minute; before: [70; 5].
        assert_eq!(fetches(&f), [50, 50, 50, 10, 10]);
    }

    /// An action reads back only what it changed.
    #[tokio::test(start_paused = true)]
    async fn an_action_reads_back_only_what_it_changed() {
        let f = Fakes::new();
        let p = platform(&f);
        let changed = fresh::Changed::default();
        state_of(p, &changed).await;
        let before = fetches(&f);
        act_on(p, &changed, HostAction::BtPower { on: true })
            .await
            .unwrap();
        tokio::time::sleep(SETTLE).await;
        state_of(p, &changed).await;
        let after = fetches(&f);
        // [audio, media, brightness, wifi, bluetooth]: only Bluetooth was read again.
        let read: Vec<u64> = after.iter().zip(before).map(|(a, b)| a - b).collect();
        assert_eq!(read, [0, 0, 0, 0, 1]);
        assert_eq!(f.log.calls(), ["bt.power true"]);
    }

    /// What a poll costs in processes on this computer: the first answer reads everything (what
    /// every poll cost before), then ten polls at the control centre's fastest pace.
    /// `cargo test -p wado host::tests::live_poll_cost -- --ignored --nocapture`.
    #[tokio::test]
    #[ignore = "runs the host's tools"]
    async fn live_poll_cost() {
        use crate::platform::linux::cmd::spawned;
        crate::clip::env::capture();
        let p = crate::platform::get();
        let changed = fresh::Changed::default();
        let t0 = (spawned(), std::time::Instant::now());
        state_of(p, &changed).await;
        let first = (spawned() - t0.0, t0.1.elapsed());
        let t1 = (spawned(), std::time::Instant::now());
        for _ in 0..10 {
            tokio::time::sleep(Duration::from_millis(1200)).await;
            state_of(p, &changed).await;
        }
        let ten = spawned() - t1.0;
        eprintln!(
            "every tool (the old cost of each poll): {} processes, {:?}; then {:.1} per poll",
            first.0,
            first.1,
            ten as f64 / 10.0
        );
    }

    #[tokio::test]
    async fn a_bad_bluetooth_address_never_reaches_the_stack() {
        let f = Fakes::new();
        let p = platform(&f);
        let r = act_on(
            p,
            &fresh::Changed::default(),
            HostAction::BtConnect {
                addr: "AA:BB; rm -rf".into(),
                connect: true,
            },
        )
        .await;
        assert_eq!(r, Err("not a Bluetooth address".into()));
        assert!(f.log.calls().is_empty());
    }

    #[tokio::test]
    async fn a_missing_capability_says_why() {
        let p: &'static Platform = Box::leak(Box::new(
            wado_platform::PlatformBuilder::empty()
                .with(Fakes::new().provider())
                .without(Capability::Wifi)
                .probe(),
        ));
        let r = act_on(p, &fresh::Changed::default(), HostAction::WifiKeep).await;
        assert_eq!(r, Ok(()), "keeping needs no Wi-Fi");
        let r = act_on(
            p,
            &fresh::Changed::default(),
            HostAction::WifiRadio {
                on: true,
                minutes: None,
            },
        )
        .await;
        assert_eq!(r, Err(wado_platform::registry::LEFT_OUT.into()));
        assert_eq!(state_of(p, &fresh::Changed::default()).await.wifi, None);
    }
}
