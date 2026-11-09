//! What the input devices ask of the Mac, as plain calls — so every rule above it is tested
//! against a recording fake, and only `cg.rs` talks to CoreGraphics.

/// A mouse button.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Button {
    Left,
    Right,
}

/// Where a two-finger scroll is: the gesture itself, or the glide after the fingers lift.
/// One or the other on any event, never both — apps read them as different things.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Began,
    Changed,
    Ended,
    MomentumBegan,
    Momentum,
    MomentumEnded,
}

/// The Dock's own gestures, which follow the fingers. The value is the Dock's motion number.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dock {
    /// Between Spaces and full-screen apps.
    Spaces = 1,
    /// Mission Control (up) and App Exposé (down).
    Mission = 2,
    /// Launchpad (pinch in) and Show Desktop (spread).
    Pinch = 3,
}

/// Where a followed gesture is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    Began,
    Changed,
    Ended,
    Cancelled,
}

/// Modifier flags, as `CGEventFlags` spells them.
pub mod flag {
    pub const SHIFT: u64 = 0x0002_0000;
    pub const CTRL: u64 = 0x0004_0000;
    pub const ALT: u64 = 0x0008_0000;
    pub const CMD: u64 = 0x0010_0000;
    pub const NUMPAD: u64 = 0x0020_0000;
    pub const FN: u64 = 0x0080_0000;
}

pub trait Poster: Send {
    /// Where the pointer is now, in global points.
    fn cursor(&mut self) -> (f64, f64);
    /// `p` kept on a display: a point off every screen stays where `prev` was on that axis.
    fn clamp(&mut self, p: (f64, f64), prev: (f64, f64)) -> (f64, f64);
    /// Move the pointer to `to`, by `(dx, dy)`; `held`: a button is down, so it is a drag.
    fn move_to(&mut self, to: (f64, f64), d: (f64, f64), held: Option<Button>);
    /// Press or release `b` at `at`; `clicks`: 1 for a click, 2 for the second of a double.
    fn button(&mut self, b: Button, down: bool, at: (f64, f64), clicks: i64);
    /// Scroll by `(dx, dy)` points at the pointer.
    fn scroll(&mut self, d: (f64, f64), phase: Phase);
    /// A key by macOS virtual key code, with exactly `flags` held.
    fn key(&mut self, kvk: u16, down: bool, flags: u64);
    /// Committed text, typed whatever the keyboard layout.
    fn text(&mut self, s: &str);
    /// A Dock gesture at `progress` (±1.0 ≈ one whole Space, or Mission Control fully open),
    /// signed as the Dock reads it; `exit`: its speed as the fingers lift.
    fn dock(&mut self, kind: Dock, progress: f64, stage: Stage, exit: f64);
    /// A two-finger pinch: `by` is the change in scale since the last one (positive: zoom in).
    fn magnify(&mut self, by: f64, stage: Stage);
}

#[cfg(test)]
pub mod fake {
    use super::*;

    /// What was posted, in order.
    #[derive(Debug, Clone, PartialEq)]
    pub enum Ev {
        Move((f64, f64), Option<Button>),
        Button(Button, bool, i64),
        Scroll((f64, f64), Phase),
        Key(u16, bool, u64),
        Text(String),
        Dock(Dock, f64, Stage),
        Magnify(f64, Stage),
    }

    pub struct Rec {
        pub at: (f64, f64),
        pub evs: Vec<Ev>,
    }

    impl Default for Rec {
        fn default() -> Self {
            Rec {
                at: (500.0, 500.0),
                evs: Vec::new(),
            }
        }
    }

    impl Rec {
        pub fn buttons(&self) -> Vec<(Button, bool, i64)> {
            self.evs
                .iter()
                .filter_map(|e| match e {
                    Ev::Button(b, d, c) => Some((*b, *d, *c)),
                    _ => None,
                })
                .collect()
        }
        pub fn phases(&self) -> Vec<Phase> {
            self.evs
                .iter()
                .filter_map(|e| match e {
                    Ev::Scroll(_, p) => Some(*p),
                    _ => None,
                })
                .collect()
        }
        pub fn docks(&self) -> Vec<(Dock, f64, Stage)> {
            self.evs
                .iter()
                .filter_map(|e| match e {
                    Ev::Dock(k, p, s) => Some((*k, *p, *s)),
                    _ => None,
                })
                .collect()
        }
        pub fn keys(&self) -> Vec<(u16, bool, u64)> {
            self.evs
                .iter()
                .filter_map(|e| match e {
                    Ev::Key(k, d, f) => Some((*k, *d, *f)),
                    _ => None,
                })
                .collect()
        }
    }

    impl Poster for Rec {
        fn cursor(&mut self) -> (f64, f64) {
            self.at
        }
        fn clamp(&mut self, p: (f64, f64), _prev: (f64, f64)) -> (f64, f64) {
            (p.0.clamp(0.0, 1920.0), p.1.clamp(0.0, 1080.0))
        }
        fn move_to(&mut self, to: (f64, f64), _d: (f64, f64), held: Option<Button>) {
            self.at = to;
            self.evs.push(Ev::Move(to, held));
        }
        fn button(&mut self, b: Button, down: bool, _at: (f64, f64), clicks: i64) {
            self.evs.push(Ev::Button(b, down, clicks));
        }
        fn scroll(&mut self, d: (f64, f64), phase: Phase) {
            self.evs.push(Ev::Scroll(d, phase));
        }
        fn key(&mut self, kvk: u16, down: bool, flags: u64) {
            self.evs.push(Ev::Key(kvk, down, flags));
        }
        fn text(&mut self, s: &str) {
            self.evs.push(Ev::Text(s.to_string()));
        }
        fn dock(&mut self, kind: Dock, progress: f64, stage: Stage, _exit: f64) {
            self.evs.push(Ev::Dock(kind, progress, stage));
        }
        fn magnify(&mut self, by: f64, stage: Stage) {
            self.evs.push(Ev::Magnify(by, stage));
        }
    }
}
