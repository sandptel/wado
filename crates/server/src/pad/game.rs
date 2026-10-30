//! The sessionless gamepad: one phone's controller on this computer, from the Gamepad screen
//! opening to the device going away.
//!
//! The device is the platform's `GamepadInjector` (on Linux a uinput Xbox 360 pad, the same kind
//! a session makes), opened on the first `GameOn` — or on the first button or axes, for a phone
//! that reconnected mid-game. Games enumerate controllers when they start, so the device stays
//! for as long as the Gamepad screen is open, not per press, and **30 s beyond** (`GameOff`, the
//! phone saying goodbye, its link dropping, a refusal): a person flipping to the touchpad and back
//! must not have the game see the controller unplugged and plugged in again. Everything held is
//! let go at once, though — a controller left over must never hold a button.
//!
//! Safety rules, the same shape as the touchpad's fingers:
//! - Axes arrive as the whole state, newest wins; a stale `seq` is dropped (after a quiet spell
//!   any number is a new start, like a reloaded page's touch frames).
//! - No axes for [`AXES_SILENCE_MS`] while any is off centre → they are centred: the phone
//!   re-sends every 100 ms while a stick is held, so a lost "back to centre" cannot leave it held.
//! - No message at all for `trackpad { key-silence-ms }` (the phone pings every 2 s) → every
//!   button is let go too.
//!
//! The d-pad's four buttons are written as the hat, and the digital triggers as the trigger axes
//! at full travel; a pedal's analog travel and a trigger button combine as the larger of the two.
//! Only what changed is written. Pure apart from the device it drives, on a clock it is handed, so
//! every rule is tested against the fake platform.

use std::collections::BTreeSet;
use std::sync::Arc;

use tracing::{debug, info};
use wado_platform::input::{GamepadEvent, GamepadInjector, InputError, Opener};
use wado_protocol::pad::{PadMsg, btn, newer};

/// How long the controller outlives the Gamepad screen.
pub const GRACE_MS: u64 = 30_000;
/// Axes held off centre with nothing heard this long are centred.
pub const AXES_SILENCE_MS: u64 = 250;

/// The device's own buttons, as `PadMsg::Btn` names them.
const BUTTONS: [u16; 11] = [
    0x130, 0x131, 0x133, 0x134, 0x136, 0x137, 0x13a, 0x13b, 0x13c, 0x13d, 0x13e,
];
/// `ABS_*` codes, in `PadMsg::Axes` order: lx, ly, rx, ry, lt, rt.
const AXES: [u16; 6] = [0x00, 0x01, 0x03, 0x04, 0x02, 0x05];
const HAT_X: u16 = 0x10;
const HAT_Y: u16 = 0x11;

pub type OpenGamepad = Arc<dyn Opener<dyn GamepadInjector>>;

pub struct Game {
    /// What makes the device — or why this computer cannot have one.
    open: Result<OpenGamepad, InputError>,
    /// The player number to report once a device is made (wado's live controllers, counted).
    count: fn() -> usize,
    key_silence_ms: u64,
    dev: Option<Box<dyn GamepadInjector>>,
    player: u8,
    /// Buttons down as written, d-pad (up, down, left, right), digital triggers (l, r).
    buttons: BTreeSet<u16>,
    dpad: [bool; 4],
    digital: [bool; 2],
    /// The last axes the phone sent: lx, ly, rx, ry, lt, rt.
    axes: [i32; 6],
    /// What the device was last told, per `ABS_*` code (all start centred).
    written: [(u16, i32); 8],
    last_seq: Option<u32>,
    last_axes_ms: u64,
    last_heard_ms: u64,
    /// The screen closed at this ms: the device goes [`GRACE_MS`] later.
    off_since: Option<u64>,
}

impl Game {
    pub fn new(
        open: Result<OpenGamepad, InputError>,
        count: fn() -> usize,
        key_silence_ms: u64,
    ) -> Game {
        Game {
            open,
            count,
            key_silence_ms,
            dev: None,
            player: 0,
            buttons: BTreeSet::new(),
            dpad: [false; 4],
            digital: [false; 2],
            axes: [0; 6],
            written: [0x00, 0x01, 0x02, 0x03, 0x04, 0x05, HAT_X, HAT_Y].map(|c| (c, 0)),
            last_seq: None,
            last_axes_ms: 0,
            last_heard_ms: 0,
            off_since: None,
        }
    }

    /// The platform's controller, looked for again when missing: `/dev/uinput` may have been
    /// fixed since the daemon started.
    pub fn from_platform(key_silence_ms: u64) -> Game {
        let p = crate::platform::get();
        if !p.has(wado_platform::Capability::Gamepad) {
            p.reprobe(wado_platform::Capability::Gamepad);
        }
        let open = p.gamepad().map_err(|a| {
            InputError::Unavailable(if cfg!(target_os = "linux") {
                a.why
            } else {
                super::sink::UNSUPPORTED.into()
            })
        });
        Game::new(
            open,
            wado_compositor::input::gamepad::live_count,
            key_silence_ms,
        )
    }

    /// A controller exists now.
    pub fn alive(&self) -> bool {
        self.dev.is_some()
    }

    /// Anything at all arrived from the phone at `now`: it is still there.
    pub fn heard(&mut self, now: u64) {
        self.last_heard_ms = now;
    }

    /// One gamepad message at `now`. The reply, if any: `Game` when the device was just made or
    /// the screen (re)opened, a fatal `Err` when it cannot be made.
    pub fn handle(&mut self, now: u64, msg: PadMsg) -> Option<PadMsg> {
        self.heard(now);
        if matches!(msg, PadMsg::GameOff) {
            self.close(now);
            return None;
        }
        let made = match self.ensure() {
            Ok(made) => made,
            Err(e) => {
                return Some(PadMsg::Err {
                    fatal: matches!(e, InputError::Unavailable(_)),
                    message: e.to_string(),
                });
            }
        };
        // Used again: the screen is open, whatever said it closed.
        self.off_since = None;
        match msg {
            PadMsg::GameOn => {
                return Some(PadMsg::Game {
                    player: self.player,
                });
            }
            PadMsg::Btn { code, down } => self.button(code, down),
            PadMsg::Axes { seq, a } => self.set_axes(now, seq, a),
            _ => {}
        }
        made.then_some(PadMsg::Game {
            player: self.player,
        })
    }

    /// The screen closed, or the link did: let go of everything now, keep the device a while.
    pub fn close(&mut self, now: u64) {
        self.release();
        if self.dev.is_some() && self.off_since.is_none() {
            self.off_since = Some(now);
        }
    }

    /// Apply the timed rules at `now`: axes centred after silence, buttons let go after total
    /// silence, the device dropped after its grace.
    pub fn tick(&mut self, now: u64) {
        if self.dev.is_none() {
            return;
        }
        if self.axes != [0; 6] && now.saturating_sub(self.last_axes_ms) > AXES_SILENCE_MS {
            self.axes = [0; 6];
            self.write_axes();
        }
        if self.held() && now.saturating_sub(self.last_heard_ms) > self.key_silence_ms {
            self.release();
        }
        if self
            .off_since
            .is_some_and(|t| now.saturating_sub(t) >= GRACE_MS)
        {
            self.release();
            self.dev = None;
            self.off_since = None;
            self.last_seq = None;
            info!("pad: gamepad removed");
        }
    }

    /// When [`Game::tick`] next has something to do, in the clock's ms; `None` when nothing is
    /// timed (no device, or one with nothing held, nothing off centre and its screen open).
    pub fn due(&self) -> Option<u64> {
        self.dev.as_ref()?;
        let axes = (self.axes != [0; 6]).then_some(self.last_axes_ms + AXES_SILENCE_MS + 1);
        let keys = self
            .held()
            .then_some(self.last_heard_ms + self.key_silence_ms + 1);
        let grace = self.off_since.map(|t| t + GRACE_MS);
        [axes, keys, grace].into_iter().flatten().min()
    }

    /// Any button, d-pad direction or trigger button down.
    fn held(&self) -> bool {
        !self.buttons.is_empty() || self.dpad.contains(&true) || self.digital.contains(&true)
    }

    /// Make the device if there is none. True when it was just made.
    fn ensure(&mut self) -> Result<bool, InputError> {
        if self.dev.is_some() {
            return Ok(false);
        }
        let open = self.open.as_ref().map_err(Clone::clone)?;
        self.dev = Some(open.open()?);
        self.player = (self.count)().clamp(1, u8::MAX as usize) as u8;
        self.reset_state();
        info!(player = self.player, "pad: gamepad made");
        Ok(true)
    }

    fn reset_state(&mut self) {
        self.buttons.clear();
        self.dpad = [false; 4];
        self.digital = [false; 2];
        self.axes = [0; 6];
        for w in &mut self.written {
            w.1 = 0;
        }
        self.last_seq = None;
    }

    fn button(&mut self, code: u16, down: bool) {
        let dpad = [
            btn::DPAD_UP,
            btn::DPAD_DOWN,
            btn::DPAD_LEFT,
            btn::DPAD_RIGHT,
        ];
        if let Some(i) = dpad.iter().position(|&c| c == code) {
            self.dpad[i] = down;
            let [u, d, l, r] = self.dpad.map(i32::from);
            self.write(HAT_X, r - l);
            self.write(HAT_Y, d - u);
        } else if let Some(i) = [btn::TL2, btn::TR2].iter().position(|&c| c == code) {
            self.digital[i] = down;
            self.write_axes();
        } else if BUTTONS.contains(&code) {
            let changed = if down {
                self.buttons.insert(code)
            } else {
                self.buttons.remove(&code)
            };
            if changed && let Some(dev) = self.dev.as_mut() {
                dev.report(GamepadEvent::Button {
                    code,
                    pressed: down,
                });
            }
        } else {
            debug!(code, "pad: not a gamepad button — ignored");
        }
    }

    fn set_axes(&mut self, now: u64, seq: u32, a: [i32; 6]) {
        // After a quiet spell with everything centred, any number is a new start.
        let restarted =
            self.axes == [0; 6] && now.saturating_sub(self.last_axes_ms) > AXES_SILENCE_MS;
        if self.last_seq.is_some_and(|last| !newer(seq, last)) && !restarted {
            return;
        }
        self.last_seq = Some(seq);
        self.last_axes_ms = now;
        let stick = |v: i32| v.clamp(-32768, 32767);
        let trig = |v: i32| v.clamp(0, 255);
        self.axes = [
            stick(a[0]),
            stick(a[1]),
            stick(a[2]),
            stick(a[3]),
            trig(a[4]),
            trig(a[5]),
        ];
        self.write_axes();
    }

    /// Every axis from the sticks and triggers as they stand; only changed ones reach the device.
    fn write_axes(&mut self) {
        let full = |on: bool| if on { 255 } else { 0 };
        let values = [
            self.axes[0],
            self.axes[1],
            self.axes[2],
            self.axes[3],
            self.axes[4].max(full(self.digital[0])),
            self.axes[5].max(full(self.digital[1])),
        ];
        for (code, v) in AXES.into_iter().zip(values) {
            self.write(code, v);
        }
    }

    fn write(&mut self, code: u16, value: i32) {
        let Some(slot) = self.written.iter_mut().find(|(c, _)| *c == code) else {
            return;
        };
        if slot.1 == value {
            return;
        }
        slot.1 = value;
        if let Some(dev) = self.dev.as_mut() {
            dev.report(GamepadEvent::Axis { code, value });
        }
    }

    /// Every button up, every axis centred.
    fn release(&mut self) {
        for code in std::mem::take(&mut self.buttons) {
            if let Some(dev) = self.dev.as_mut() {
                dev.report(GamepadEvent::Button {
                    code,
                    pressed: false,
                });
            }
        }
        self.dpad = [false; 4];
        self.digital = [false; 2];
        self.axes = [0; 6];
        self.write(HAT_X, 0);
        self.write(HAT_Y, 0);
        self.write_axes();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wado_platform::fake::input::{InputCall as Call, InputLog};

    fn game() -> (Game, InputLog) {
        let log = InputLog::default();
        (Game::new(Ok(log.gamepad()), || 1, 5000), log)
    }
    fn btn_(code: u16, down: bool) -> PadMsg {
        PadMsg::Btn { code, down }
    }
    fn axes(seq: u32, a: [i32; 6]) -> PadMsg {
        PadMsg::Axes { seq, a }
    }
    fn b(code: u16, pressed: bool) -> Call {
        Call::Gamepad(GamepadEvent::Button { code, pressed })
    }
    fn ax(code: u16, value: i32) -> Call {
        Call::Gamepad(GamepadEvent::Axis { code, value })
    }
    fn opens(log: &InputLog) -> usize {
        log.calls()
            .iter()
            .filter(|c| **c == Call::OpenGamepad)
            .count()
    }

    #[test]
    fn game_on_makes_the_device_once_and_says_which_player() {
        let (mut g, log) = game();
        assert_eq!(
            g.handle(0, PadMsg::GameOn),
            Some(PadMsg::Game { player: 1 })
        );
        assert_eq!(
            g.handle(10, PadMsg::GameOn),
            Some(PadMsg::Game { player: 1 }),
            "asked again, answered again"
        );
        assert_eq!(opens(&log), 1, "made once");
        assert!(log.io().is_empty(), "nothing pressed yet");
    }

    #[test]
    fn the_player_is_the_count_after_opening() {
        let log = InputLog::default();
        let mut g = Game::new(Ok(log.gamepad()), || 2, 5000);
        assert_eq!(
            g.handle(0, PadMsg::GameOn),
            Some(PadMsg::Game { player: 2 })
        );
    }

    #[test]
    fn a_press_before_game_on_makes_the_device_too() {
        let (mut g, log) = game();
        assert_eq!(
            g.handle(0, btn_(0x130, true)),
            Some(PadMsg::Game { player: 1 })
        );
        assert_eq!(g.handle(1, btn_(0x130, false)), None);
        assert_eq!(log.io(), vec![b(0x130, true), b(0x130, false)]);
    }

    #[test]
    fn buttons_are_written_once_per_change_and_unknown_codes_ignored() {
        let (mut g, log) = game();
        g.handle(0, PadMsg::GameOn);
        g.handle(1, btn_(0x131, true));
        g.handle(2, btn_(0x131, true));
        g.handle(3, btn_(0x999, true));
        g.handle(4, btn_(0x131, false));
        assert_eq!(log.io(), vec![b(0x131, true), b(0x131, false)]);
    }

    #[test]
    fn the_dpad_is_the_hat_and_combines() {
        let (mut g, log) = game();
        g.handle(0, PadMsg::GameOn);
        g.handle(1, btn_(btn::DPAD_UP, true));
        g.handle(2, btn_(btn::DPAD_LEFT, true));
        g.handle(3, btn_(btn::DPAD_RIGHT, true)); // left + right = no x
        g.handle(4, btn_(btn::DPAD_UP, false));
        assert_eq!(
            log.io(),
            vec![ax(HAT_Y, -1), ax(HAT_X, -1), ax(HAT_X, 0), ax(HAT_Y, 0)]
        );
    }

    #[test]
    fn a_trigger_is_the_larger_of_its_button_and_its_pedal() {
        let (mut g, log) = game();
        g.handle(0, PadMsg::GameOn);
        g.handle(1, axes(1, [0, 0, 0, 0, 100, 0]));
        g.handle(2, btn_(btn::TL2, true));
        g.handle(3, axes(2, [0, 0, 0, 0, 120, 0]));
        g.handle(4, btn_(btn::TL2, false));
        g.handle(5, btn_(btn::TR2, true));
        assert_eq!(
            log.io(),
            vec![ax(0x02, 100), ax(0x02, 255), ax(0x02, 120), ax(0x05, 255)]
        );
    }

    #[test]
    fn axes_are_clamped_changed_only_and_stale_ones_dropped() {
        let (mut g, log) = game();
        g.handle(0, PadMsg::GameOn);
        g.handle(1, axes(5, [40000, -100, 0, 0, 300, 0]));
        g.handle(2, axes(4, [1, 1, 1, 1, 1, 1])); // older: dropped
        g.handle(3, axes(6, [40000, -100, 7, 0, 300, 0])); // only rx changed
        assert_eq!(
            log.io(),
            vec![ax(0x00, 32767), ax(0x01, -100), ax(0x02, 255), ax(0x03, 7)]
        );
    }

    #[test]
    fn a_restarted_count_is_taken_after_a_quiet_spell() {
        let (mut g, log) = game();
        g.handle(0, PadMsg::GameOn);
        g.handle(1, axes(500, [10, 0, 0, 0, 0, 0]));
        g.handle(2, axes(501, [0; 6]));
        g.handle(1000, axes(1, [20, 0, 0, 0, 0, 0])); // a reloaded page
        assert_eq!(log.io(), vec![ax(0, 10), ax(0, 0), ax(0, 20)]);
    }

    #[test]
    fn silence_centres_held_axes_once() {
        let (mut g, log) = game();
        g.handle(0, PadMsg::GameOn);
        g.handle(10, axes(1, [1000, 0, 0, 0, 50, 0]));
        assert_eq!(g.due(), Some(10 + AXES_SILENCE_MS + 1));
        g.tick(200);
        assert_eq!(log.io().len(), 2, "still within the silence");
        g.tick(10 + AXES_SILENCE_MS + 1);
        assert_eq!(log.io()[2..], [ax(0x00, 0), ax(0x02, 0)]);
        g.tick(2000);
        assert_eq!(log.io().len(), 4, "centred once");
    }

    #[test]
    fn total_silence_lets_go_of_buttons() {
        let (mut g, log) = game();
        g.handle(0, PadMsg::GameOn);
        g.handle(1, btn_(0x130, true));
        g.handle(2, btn_(btn::DPAD_DOWN, true));
        g.tick(4000);
        assert_eq!(log.io().len(), 2);
        g.tick(5003);
        assert_eq!(log.io()[2..], [b(0x130, false), ax(HAT_Y, 0)]);
    }

    #[test]
    fn game_off_lets_go_now_and_the_device_goes_after_the_grace() {
        let (mut g, log) = game();
        g.handle(0, PadMsg::GameOn);
        g.handle(1, btn_(0x134, true));
        g.handle(2, axes(1, [0, 0, -5, 0, 0, 0]));
        assert_eq!(g.handle(3, PadMsg::GameOff), None);
        assert_eq!(log.io()[2..], [b(0x134, false), ax(0x03, 0)]);
        assert_eq!(g.due(), Some(3 + GRACE_MS));
        g.tick(3 + GRACE_MS - 1);
        assert!(g.alive(), "kept while the grace runs");
        g.tick(3 + GRACE_MS);
        assert!(!g.alive(), "then removed");
        assert_eq!(g.due(), None);
        // Opened again later: a new device.
        assert_eq!(
            g.handle(40_000, PadMsg::GameOn),
            Some(PadMsg::Game { player: 1 })
        );
        assert_eq!(opens(&log), 2);
    }

    #[test]
    fn coming_back_within_the_grace_keeps_the_same_device() {
        let (mut g, log) = game();
        g.handle(0, PadMsg::GameOn);
        g.handle(5, PadMsg::GameOff);
        g.tick(20_000);
        g.handle(20_001, PadMsg::GameOn);
        g.tick(5 + GRACE_MS + 10);
        assert!(g.alive(), "the grace was cancelled");
        assert_eq!(opens(&log), 1);
        // And a link that drops starts it again.
        g.close(50_000);
        g.tick(50_000 + GRACE_MS);
        assert!(!g.alive());
    }

    #[test]
    fn a_computer_without_the_device_says_why_and_it_is_fatal() {
        let log = InputLog::default();
        log.refuse(InputError::Unavailable(
            "/dev/uinput: permission denied".into(),
        ));
        let mut g = Game::new(Ok(log.gamepad()), || 1, 5000);
        assert_eq!(
            g.handle(0, PadMsg::GameOn),
            Some(PadMsg::Err {
                message: "/dev/uinput: permission denied".into(),
                fatal: true
            })
        );
        let mut absent = Game::new(Err(InputError::Unavailable("no uinput".into())), || 1, 5000);
        assert!(matches!(
            absent.handle(0, btn_(0x130, true)),
            Some(PadMsg::Err { fatal: true, .. })
        ));
        assert!(!absent.alive());
    }
}
