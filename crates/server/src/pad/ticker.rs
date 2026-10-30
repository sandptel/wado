//! The pad's tick, and when it may rest.
//!
//! While anything is in flight — a finger down, a frame queued for the pacer, the pacer's delay
//! still settling — the pad is ticked every `trackpad { rate-hz }`, exactly as a touchpad
//! reports. When nothing is, a tick would change nothing, so the ticker parks: it waits for the
//! next message (`wake`, signalled after each one is applied) or for the pad's next timed rule
//! (held keys let go at key silence, the arrivals summary), whichever comes first, and ticks the
//! moment it wakes, then on the cadence again. An idle open pad costs a lock per message instead
//! of `rate-hz` locks a second.

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Notify;
use tokio::time::{Instant, MissedTickBehavior};

/// What the pad needs from its tick next (`Pad::pace`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pace {
    /// Something is in flight: tick again on the cadence.
    Tick,
    /// Nothing to do until a message arrives, or until this pad-clock ms, when a timed rule
    /// falls due.
    Park(Option<u64>),
}

/// Tick every `period` until aborted. `step` locks the pad, ticks it at its clock (ms since
/// `epoch`) and says what next; `wake` is signalled after every message the pad takes.
pub async fn run(
    period: Duration,
    epoch: Instant,
    wake: Arc<Notify>,
    mut step: impl FnMut() -> Pace,
) {
    let mut every = tokio::time::interval(period);
    every.set_missed_tick_behavior(MissedTickBehavior::Skip);
    loop {
        every.tick().await;
        let Pace::Park(until) = step() else {
            continue;
        };
        // A message that landed since `step` left a permit, so this returns at once.
        match until {
            Some(ms) => tokio::select! {
                () = wake.notified() => {}
                () = tokio::time::sleep_until(epoch + Duration::from_millis(ms)) => {}
            },
            None => wake.notified().await,
        }
        every.reset_immediately();
    }
}

/// The pad clock: ms since `epoch`.
pub fn ms_since(epoch: Instant) -> u64 {
    epoch.elapsed().as_millis() as u64
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use wado_config::schema::Trackpad;
    use wado_config::schema::trackpad::TrackpadDelay;
    use wado_protocol::pad::{Contact, Mod, PadMsg};

    use wado_platform::input::{InputRes, KeyInjector, TouchInjector};

    use super::*;
    use crate::pad::pacer::period;
    use crate::pad::sink::Devices;
    use crate::pad::state::Pad;

    #[derive(Debug, Clone, PartialEq)]
    enum Call {
        Touch(Vec<Contact>),
        Hold(Vec<Mod>),
        Release,
    }

    #[derive(Clone, Default)]
    struct Rec(Arc<Mutex<Vec<(u64, Call)>>>, Option<Instant>);
    impl Rec {
        fn calls(&self) -> Vec<Call> {
            self.0.lock().unwrap().iter().map(|c| c.1.clone()).collect()
        }
        fn at(&self, call: &Call) -> Option<u64> {
            self.0
                .lock()
                .unwrap()
                .iter()
                .find(|c| c.1 == *call)
                .map(|c| c.0)
        }
        fn push(&self, call: Call) {
            let now = self.1.map_or(0, ms_since);
            self.0.lock().unwrap().push((now, call));
        }
    }
    impl TouchInjector for Rec {
        fn resize(&mut self, _: u16, _: u16) -> InputRes {
            Ok(())
        }
        fn frame(&mut self, c: &[Contact]) -> InputRes {
            self.push(Call::Touch(c.to_vec()));
            Ok(())
        }
    }
    impl KeyInjector for Rec {
        fn tap(&mut self, _: &[Mod], _: u16) -> InputRes {
            Ok(())
        }
        fn hold(&mut self, mods: &[Mod]) -> InputRes {
            self.push(Call::Hold(mods.to_vec()));
            Ok(())
        }
        fn release_all(&mut self) -> InputRes {
            self.push(Call::Release);
            Ok(())
        }
    }
    impl Rec {
        /// Devices that record into this.
        fn devices(&self) -> Devices {
            let (t, k) = (self.clone(), self.clone());
            Devices::new(
                Arc::new(move || Ok(Box::new(t.clone()) as Box<dyn TouchInjector>)),
                Arc::new(move || Ok(Box::new(k.clone()) as Box<dyn KeyInjector>)),
            )
        }
    }

    /// A pad on the ticker as `live.rs` runs it, at the default 125 Hz, counting how often the
    /// tick takes the pad's lock. `park: false` is the ticker before it could park.
    struct Rig {
        pad: Arc<Mutex<Pad>>,
        rec: Rec,
        wake: Arc<Notify>,
        epoch: Instant,
        locks: Arc<AtomicUsize>,
    }

    impl Rig {
        fn new(cfg: &Trackpad, park: bool) -> Rig {
            let epoch = Instant::now();
            let rec = Rec(Arc::default(), Some(epoch));
            let mut pad = Pad::with_config(rec.devices(), cfg);
            pad.handle(0, PadMsg::Size { w: 390, h: 800 }, false);
            let pad = Arc::new(Mutex::new(pad));
            let wake = Arc::new(Notify::new());
            let locks = Arc::new(AtomicUsize::new(0));
            let step = {
                let (pad, locks) = (pad.clone(), locks.clone());
                move || {
                    locks.fetch_add(1, Ordering::SeqCst);
                    let mut pad = pad.lock().unwrap();
                    pad.tick(ms_since(epoch));
                    if park { pad.pace() } else { Pace::Tick }
                }
            };
            tokio::spawn(run(period(125), epoch, wake.clone(), step));
            Rig {
                pad,
                rec,
                wake,
                epoch,
                locks,
            }
        }

        /// A message from the phone, as `Live::accept` takes it: applied, then the tick woken.
        fn send(&self, msg: PadMsg, relayed: bool) {
            let now = ms_since(self.epoch);
            self.pad.lock().unwrap().handle(now, msg, relayed);
            self.wake.notify_one();
        }

        fn locks(&self) -> usize {
            self.locks.load(Ordering::SeqCst)
        }
    }

    /// Let every task that can run, run, without moving the clock.
    async fn settle() {
        for _ in 0..10 {
            tokio::task::yield_now().await;
        }
    }

    async fn until(rig: &Rig, ms: u64) {
        tokio::time::sleep_until(rig.epoch + Duration::from_millis(ms)).await;
        settle().await;
    }

    fn frame(seq: u32, t: u32, c: Vec<Contact>) -> PadMsg {
        PadMsg::Frame { seq, t, c }
    }

    #[tokio::test(start_paused = true)]
    async fn an_idle_pad_parks_instead_of_locking_every_tick() {
        let cfg = Trackpad::default();
        let before = Rig::new(&cfg, false);
        let after = Rig::new(&cfg, true);
        // Ten seconds of an open pad, nothing touched, the phone pinging every 2 s.
        for at in (2000..=10_000).step_by(2000) {
            until(&after, at).await;
            before.send(
                PadMsg::Ping {
                    t: at as u32,
                    bin: false,
                },
                false,
            );
            after.send(
                PadMsg::Ping {
                    t: at as u32,
                    bin: false,
                },
                false,
            );
        }
        until(&after, 10_000).await;
        let (b, a) = (before.locks(), after.locks());
        println!("pad lock acquisitions by the tick over 10 s idle: {b} before, {a} parked");
        assert!(b >= 1250, "the old cadence: {b}");
        assert!(a <= 1 + 5 * 2, "parked: {a}");
    }

    #[tokio::test(start_paused = true)]
    async fn a_parked_pad_still_lets_go_of_held_keys_at_key_silence() {
        let cfg = Trackpad::default();
        let silence = cfg.key_silence_ms;
        let rig = Rig::new(&cfg, true);
        until(&rig, 100).await;
        rig.send(
            PadMsg::Hold {
                mods: vec![Mod::Super],
            },
            false,
        );
        until(&rig, 100 + silence).await;
        assert!(!rig.rec.calls().contains(&Call::Release), "not yet");
        until(&rig, 100 + silence + 2).await;
        assert_eq!(rig.rec.at(&Call::Release), Some(100 + silence + 1));
        until(&rig, 100 + 3 * silence).await;
        assert_eq!(
            rig.rec.calls(),
            vec![Call::Hold(vec![Mod::Super]), Call::Release],
            "let go once"
        );
        assert!(rig.locks() <= 4, "parked throughout: {}", rig.locks());
    }

    #[tokio::test(start_paused = true)]
    async fn the_first_frame_after_idle_is_applied_without_waiting_for_a_tick() {
        // Played with no delay, so a tick that runs the moment the frame lands plays it.
        let cfg = Trackpad {
            delay: TrackpadDelay::Fixed,
            direct_delay_ms: 0,
            ..Default::default()
        };
        let rig = Rig::new(&cfg, true);
        until(&rig, 1003).await; // between two 8 ms ticks of the old cadence (1000, 1008)
        let parked = rig.locks();
        rig.send(frame(1, 5000, vec![(0, 1, 1)]), false);
        settle().await;
        assert_eq!(rig.rec.at(&Call::Touch(vec![(0, 1, 1)])), Some(1003));
        assert_eq!(rig.locks(), parked + 1);
    }

    #[tokio::test(start_paused = true)]
    async fn a_finger_down_keeps_the_cadence_and_silence_still_lifts_it() {
        let rig = Rig::new(&Trackpad::default(), true);
        until(&rig, 1000).await;
        rig.send(frame(1, 0, vec![(0, 1, 1)]), false);
        until(&rig, 1100).await;
        assert_eq!(rig.rec.calls(), vec![Call::Touch(vec![(0, 1, 1)])]);
        let ticking = rig.locks();
        until(&rig, 1200).await;
        assert!(
            rig.locks() >= ticking + 12,
            "on the 8 ms cadence while down"
        );
        // The phone went quiet: the finger lifts on the first tick past 400 ms of silence.
        until(&rig, 1400).await;
        assert_eq!(rig.rec.calls().len(), 1, "not before 400 ms");
        until(&rig, 1409).await;
        let lift = rig.rec.at(&Call::Touch(vec![]));
        assert!(
            lift.is_some_and(|at| at > 1400 && at <= 1409),
            "lifted at {lift:?}"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn a_relayed_frame_wakes_a_parked_pad_and_still_plays_out_behind() {
        let rig = Rig::new(&Trackpad::default(), true);
        until(&rig, 2000).await;
        rig.send(frame(1, 0, vec![(0, 1, 1)]), true);
        until(&rig, 2023).await;
        assert!(
            rig.rec.calls().is_empty(),
            "the relay's 24 ms playout delay holds it"
        );
        until(&rig, 2024).await;
        assert_eq!(
            rig.rec.at(&Call::Touch(vec![(0, 1, 1)])),
            Some(2024),
            "on the cadence the wake restarted"
        );
    }
}
