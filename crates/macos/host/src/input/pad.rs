//! The phone's touchpad, read as a Mac trackpad would be.
//!
//! macOS has no public way to inject touches, so the fingers are interpreted here and posted as
//! what a trackpad produces: pointer moves, clicks, drags, scrolling with its phases and glide,
//! pinch-to-zoom, and the Dock's own gestures, which follow the fingers as they move.
//!
//! - One finger moves the pointer; a quick tap clicks; tap, then touch again and move: a drag.
//! - Two fingers scroll (natural: the content follows the fingers), then glide; a quick
//!   two-finger tap is a right click; spreading or pinching zooms.
//! - Three and four fingers do what this Mac's Trackpad settings say ([`super::prefs`]):
//!   left/right between Spaces (or pages), up Mission Control, down App Exposé; four fingers
//!   pinching in Launchpad, spreading Show Desktop.
//!
//! Pure: it is handed the time and a [`Poster`], so every rule is tested without a Mac.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use wado_protocol::pad::Contact;

use super::post::{Button, Dock, Phase, Poster, Stage, flag};
use super::prefs::{Horiz, Settings};

/// A touch shorter than this, that moved less than [`TAP_SLOP`], is a tap.
pub const TAP_MS: u64 = 180;
/// How far a tap may wander, in pad px.
pub const TAP_SLOP: f64 = 8.0;
/// A touch this soon after a tap starts a drag (or, without moving, a double click).
pub const DRAG_WINDOW_MS: u64 = 250;
/// Two clicks this close are a double click.
pub const DOUBLE_MS: u64 = 350;
/// Pointer points per pad px at rest; how fast acceleration grows with speed (per pad px/ms);
/// and the most it multiplies by. A phone's pad is far smaller than a Mac's, so it runs hotter.
pub const GAIN: f64 = 2.4;
pub const ACCEL: f64 = 1.5;
pub const ACCEL_MAX: f64 = 3.5;
/// How much of each frame's speed enters the smoothed speed acceleration reads.
pub const SPEED_SMOOTH: f64 = 0.3;
/// Scroll points per pad px.
pub const SCROLL_GAIN: f64 = 1.0;
/// Glide: per-16 ms decay, and the speed (points/ms) under which it stops.
pub const GLIDE_DECAY: f64 = 0.95;
pub const GLIDE_STOP: f64 = 0.02;
/// A glide starts only from at least this speed (points/ms).
pub const GLIDE_START: f64 = 0.3;
/// Three or more fingers decide what they are after this much motion, in pad px.
pub const GESTURE_SLOP: f64 = 10.0;
/// A swipe between pages fires after this share of the pad.
pub const SWIPE_SHARE: f64 = 0.25;
/// A Dock gesture's whole way (one Space, Mission Control fully open) over this share of the
/// pad; a pinch's over half of it.
pub const DOCK_SHARE: f64 = 0.5;

/// What the current touch is doing.
#[derive(Debug, Clone, PartialEq)]
enum Gesture {
    Idle,
    /// One finger: pointer, or a drag when it followed a tap.
    Point {
        drag: DragState,
    },
    /// Two fingers, before they have decided between scrolling and zooming.
    Two,
    Scroll,
    /// Two fingers zooming; `last` their spread at the last step.
    Zoom {
        last: f64,
    },
    /// Three or more fingers, before they have decided.
    Many {
        from: (f64, f64),
        spread: f64,
    },
    /// A Dock gesture under way: how far along, and the last step (its direction at the end).
    Dock {
        kind: Dock,
        progress: f64,
        last: f64,
    },
    /// A swipe between pages; `fired` once it has gone back or forward.
    Pages {
        from: (f64, f64),
        fired: bool,
    },
    /// Some fingers lifted mid-gesture: nothing more until all are up.
    Done,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum DragState {
    /// A plain touch.
    None,
    /// Touched soon after a tap: a move makes it a drag, a lift a double click.
    Armed,
    /// The button is down.
    Dragging,
}

pub struct Interpreter {
    /// Pad px per contact unit, from the pad's size.
    scale: (f64, f64),
    size: (f64, f64),
    fingers: BTreeMap<u8, (f64, f64)>,
    gesture: Gesture,
    /// When this touch began, how far it moved, and the most fingers it had.
    began: Instant,
    moved: f64,
    most: usize,
    last_tap: Option<Instant>,
    last_click: Option<Instant>,
    /// Scroll speed (points/ms), smoothed, and when the last scroll step was.
    vel: (f64, f64),
    /// Pointer speed (pad px/ms), smoothed: one late frame must not spike the acceleration.
    speed: f64,
    last_step: Option<Instant>,
    glide: Option<Glide>,
    /// Fractions of a point not yet scrolled, carried so slow scrolls are not lost.
    rest: (f64, f64),
    /// Where two fingers started, and how far apart: what tells a scroll from a pinch.
    two_from: ((f64, f64), f64),
    /// What three and four fingers do on this Mac.
    pub prefs: Settings,
}

#[derive(Debug, Clone, Copy)]
struct Glide {
    vel: (f64, f64),
    at: Instant,
    started: bool,
}

impl Interpreter {
    pub fn new(now: Instant) -> Interpreter {
        let mut i = Interpreter {
            scale: (0.0, 0.0),
            size: (0.0, 0.0),
            fingers: BTreeMap::new(),
            gesture: Gesture::Idle,
            began: now,
            moved: 0.0,
            most: 0,
            last_tap: None,
            last_click: None,
            vel: (0.0, 0.0),
            speed: 0.0,
            last_step: None,
            glide: None,
            rest: (0.0, 0.0),
            two_from: ((0.0, 0.0), 0.0),
            prefs: Settings::default(),
        };
        i.resize(400, 300);
        i
    }

    /// The pad is `w` × `h` CSS px.
    pub fn resize(&mut self, w: u16, h: u16) {
        let (w, h) = (f64::from(w.max(1)), f64::from(h.max(1)));
        self.size = (w, h);
        self.scale = (w / 65535.0, h / 65535.0);
    }

    /// A glide is still running and wants [`Interpreter::tick`].
    pub fn gliding(&self) -> bool {
        self.glide.is_some()
    }

    /// The fingers down now.
    pub fn frame(&mut self, now: Instant, contacts: &[Contact], out: &mut dyn Poster) {
        let next: BTreeMap<u8, (f64, f64)> = contacts
            .iter()
            .map(|&(id, x, y)| {
                (
                    id,
                    (f64::from(x) * self.scale.0, f64::from(y) * self.scale.1),
                )
            })
            .collect();
        let (was, now_n) = (self.fingers.len(), next.len());

        if was == 0 && now_n > 0 {
            self.start(now, &next, out);
        } else if now_n == 0 {
            if was > 0 {
                self.end(now, out);
            }
        } else if now_n > was {
            self.more_fingers(now_n, &next);
        } else if now_n < was {
            self.fewer_fingers(now, out);
        } else {
            self.moved_by(now, &next, out);
        }
        self.fingers = next;
    }

    /// Advance a glide.
    pub fn tick(&mut self, now: Instant, out: &mut dyn Poster) {
        let Some(mut g) = self.glide else { return };
        let dt = now.saturating_duration_since(g.at).as_secs_f64() * 1000.0;
        if dt <= 0.0 {
            return;
        }
        let decay = GLIDE_DECAY.powf(dt / 16.0);
        g.vel = (g.vel.0 * decay, g.vel.1 * decay);
        g.at = now;
        if g.vel.0.hypot(g.vel.1) < GLIDE_STOP {
            if g.started {
                out.scroll((0.0, 0.0), Phase::MomentumEnded);
            }
            self.glide = None;
            return;
        }
        let d = self.carry((g.vel.0 * dt, g.vel.1 * dt));
        out.scroll(
            d,
            if g.started {
                Phase::Momentum
            } else {
                Phase::MomentumBegan
            },
        );
        g.started = true;
        self.glide = Some(g);
    }

    /// Lift everything at once: a dropped link or a closed pad.
    pub fn lift_all(&mut self, now: Instant, out: &mut dyn Poster) {
        self.frame(now, &[], out);
        self.stop_glide(out);
    }

    fn stop_glide(&mut self, out: &mut dyn Poster) {
        if let Some(g) = self.glide.take()
            && g.started
        {
            out.scroll((0.0, 0.0), Phase::MomentumEnded);
        }
    }

    fn start(&mut self, now: Instant, next: &BTreeMap<u8, (f64, f64)>, out: &mut dyn Poster) {
        self.stop_glide(out);
        self.began = now;
        self.moved = 0.0;
        self.most = next.len();
        self.vel = (0.0, 0.0);
        self.speed = 0.0;
        self.last_step = None;
        self.rest = (0.0, 0.0);
        self.gesture = match next.len() {
            1 => {
                let armed = self
                    .last_tap
                    .is_some_and(|t| now.saturating_duration_since(t) < ms(DRAG_WINDOW_MS));
                Gesture::Point {
                    drag: if armed {
                        DragState::Armed
                    } else {
                        DragState::None
                    },
                }
            }
            2 => {
                self.two_from = (centroid(next), spread(next));
                Gesture::Two
            }
            _ => Gesture::Many {
                from: centroid(next),
                spread: spread(next),
            },
        };
    }

    fn more_fingers(&mut self, n: usize, next: &BTreeMap<u8, (f64, f64)>) {
        self.most = self.most.max(n);
        // A second or third finger joining early changes what the touch is; once a gesture is
        // under way, it stays what it was.
        let early = self.moved < TAP_SLOP;
        self.gesture = match (&self.gesture, n) {
            (
                Gesture::Point {
                    drag: DragState::None,
                },
                2,
            ) if early => {
                self.two_from = (centroid(next), spread(next));
                Gesture::Two
            }
            (
                Gesture::Point {
                    drag: DragState::None,
                }
                | Gesture::Two
                | Gesture::Many { .. },
                _,
            ) if n >= 3 && early => Gesture::Many {
                from: centroid(next),
                spread: spread(next),
            },
            (g, _) => g.clone(),
        };
    }

    fn fewer_fingers(&mut self, now: Instant, out: &mut dyn Poster) {
        match self.gesture {
            Gesture::Scroll => {
                out.scroll((0.0, 0.0), Phase::Ended);
                self.begin_glide(now);
                self.gesture = Gesture::Done;
            }
            Gesture::Point {
                drag: DragState::Dragging,
            } => {}
            Gesture::Point { .. } => {}
            // Fingers lift one after the other: a two-finger tap is still a tap until the last.
            Gesture::Two => {}
            _ => {
                self.finish(out);
                self.gesture = Gesture::Done;
            }
        }
    }

    /// End a Dock gesture or a zoom. A Dock gesture whose last step went back the way it came
    /// is cancelled, so the Space or Mission Control snaps back as a trackpad's does.
    fn finish(&mut self, out: &mut dyn Poster) {
        match self.gesture {
            Gesture::Dock {
                kind,
                progress,
                last,
            } => {
                let stage = if last.signum() == progress.signum() {
                    Stage::Ended
                } else {
                    Stage::Cancelled
                };
                out.dock(kind, progress, stage, last * 100.0);
            }
            Gesture::Zoom { .. } => out.magnify(0.0, Stage::Ended),
            _ => {}
        }
    }

    /// How far `d` (the fingers' mean motion) and `grew` (their spread's change) move `kind`.
    fn dock_step(&self, kind: Dock, d: (f64, f64), grew: f64) -> f64 {
        let (w, h) = self.size;
        match kind {
            // The Dock reads all three the other way round from the fingers (measured on macOS
            // 26: negative opens Mission Control, goes a Space right, and opens Show Desktop).
            Dock::Spaces => -d.0 / (w * DOCK_SHARE),
            Dock::Mission => d.1 / (h * DOCK_SHARE),
            Dock::Pinch => -grew / (w * DOCK_SHARE * 0.5),
        }
    }

    fn moved_by(&mut self, now: Instant, next: &BTreeMap<u8, (f64, f64)>, out: &mut dyn Poster) {
        let d = mean_delta(&self.fingers, next);
        let dist = d.0.hypot(d.1);
        self.moved += dist;
        match self.gesture.clone() {
            Gesture::Point { drag } => {
                let drag = if drag == DragState::Armed && self.moved > 3.0 {
                    let at = out.cursor();
                    out.button(Button::Left, true, at, 1);
                    DragState::Dragging
                } else {
                    drag
                };
                self.gesture = Gesture::Point { drag };
                self.point(now, d, drag == DragState::Dragging, out);
            }
            Gesture::Two if next.len() == 2 => {
                // Decided on the first real motion: a zoom if the fingers spread or closed more
                // than they travelled together, else a scroll.
                let (c0, s0) = self.two_from;
                let c = centroid(next);
                let travel = (c.0 - c0.0).hypot(c.1 - c0.1);
                let change = (spread(next) - s0).abs();
                self.moved = self.moved.max(travel.max(change));
                if change >= TAP_SLOP && change > travel {
                    out.magnify(0.0, Stage::Began);
                    self.gesture = Gesture::Zoom { last: s0 };
                    self.moved_by(now, next, out);
                } else if travel >= TAP_SLOP {
                    out.scroll(self.carry(scaled(d, SCROLL_GAIN)), Phase::Began);
                    self.step(now, scaled(d, SCROLL_GAIN));
                    self.gesture = Gesture::Scroll;
                }
            }
            Gesture::Scroll => {
                let s = scaled(d, SCROLL_GAIN);
                out.scroll(self.carry(s), Phase::Changed);
                self.step(now, s);
            }
            Gesture::Zoom { last } => {
                let now_s = spread(next);
                let by = now_s / last.max(1.0) - 1.0;
                if by != 0.0 {
                    out.magnify(by, Stage::Changed);
                    self.gesture = Gesture::Zoom { last: now_s };
                }
            }
            Gesture::Many { from, spread: s0 } => {
                let n = next.len();
                let c = centroid(next);
                let (dx, dy) = (c.0 - from.0, c.1 - from.1);
                let travel = dx.hypot(dy);
                let grew = spread(next) - s0;
                self.moved = self.moved.max(travel.max(grew.abs()));
                if travel.max(grew.abs()) < GESTURE_SLOP {
                    return;
                }
                let p = self.prefs;
                let kind = if n >= 4 && p.pinch && grew.abs() > travel {
                    Some(Dock::Pinch)
                } else if dx.abs() >= dy.abs() {
                    match p.horiz(n) {
                        Horiz::Spaces => Some(Dock::Spaces),
                        Horiz::Pages => {
                            self.gesture = Gesture::Pages { from, fired: false };
                            return self.moved_by(now, next, out);
                        }
                        Horiz::Off => None,
                    }
                } else {
                    p.vert(n).then_some(Dock::Mission)
                };
                self.gesture = match kind {
                    Some(kind) => {
                        let first = self.dock_step(kind, (dx, dy), grew);
                        out.dock(kind, first, Stage::Began, 0.0);
                        Gesture::Dock {
                            kind,
                            progress: first,
                            last: first,
                        }
                    }
                    None => Gesture::Done,
                };
            }
            Gesture::Dock { kind, progress, .. } => {
                let step = self.dock_step(kind, d, spread(next) - spread(&self.fingers));
                if step != 0.0 {
                    let progress = progress + step;
                    out.dock(kind, progress, Stage::Changed, 0.0);
                    self.gesture = Gesture::Dock {
                        kind,
                        progress,
                        last: step,
                    };
                }
            }
            Gesture::Pages { from, fired: false } => {
                let dx = centroid(next).0 - from.0;
                if dx.abs() >= self.size.0 * SWIPE_SHARE {
                    // ⌘[ back when the fingers go right (the page follows them), ⌘] forward.
                    chord(out, if dx > 0.0 { 33 } else { 30 }, flag::CMD);
                    self.gesture = Gesture::Pages { from, fired: true };
                }
            }
            Gesture::Two | Gesture::Pages { .. } | Gesture::Idle | Gesture::Done => {}
        }
    }

    fn point(&mut self, now: Instant, d: (f64, f64), held: bool, out: &mut dyn Poster) {
        let dt = self
            .last_step
            .map_or(16.0, |t| {
                now.saturating_duration_since(t).as_secs_f64() * 1000.0
            })
            .max(1.0);
        self.last_step = Some(now);
        // Smoothed over about four frames, as a trackpad's own acceleration is: a frame that
        // lands late carries a burst of motion in one step, and its raw speed would multiply it.
        self.speed += (d.0.hypot(d.1) / dt - self.speed) * SPEED_SMOOTH;
        let gain = GAIN * (1.0 + ACCEL * self.speed).min(ACCEL_MAX);
        let delta = (d.0 * gain, d.1 * gain);
        let from = out.cursor();
        let to = out.clamp((from.0 + delta.0, from.1 + delta.1), from);
        out.move_to(to, delta, held.then_some(Button::Left));
    }

    /// Track scroll speed for the glide.
    fn step(&mut self, now: Instant, s: (f64, f64)) {
        if let Some(t) = self.last_step {
            let dt = now.saturating_duration_since(t).as_secs_f64() * 1000.0;
            if dt > 0.0 {
                let v = (s.0 / dt, s.1 / dt);
                self.vel = (self.vel.0 * 0.5 + v.0 * 0.5, self.vel.1 * 0.5 + v.1 * 0.5);
            }
        }
        self.last_step = Some(now);
    }

    fn begin_glide(&mut self, now: Instant) {
        let fresh = self
            .last_step
            .is_some_and(|t| now.saturating_duration_since(t) < ms(60));
        if fresh && self.vel.0.hypot(self.vel.1) >= GLIDE_START {
            self.glide = Some(Glide {
                vel: self.vel,
                at: now,
                started: false,
            });
        }
    }

    /// Whole points to scroll now, keeping the fraction for the next step.
    fn carry(&mut self, d: (f64, f64)) -> (f64, f64) {
        let x = d.0 + self.rest.0;
        let y = d.1 + self.rest.1;
        let whole = (x.trunc(), y.trunc());
        self.rest = (x - whole.0, y - whole.1);
        whole
    }

    fn end(&mut self, now: Instant, out: &mut dyn Poster) {
        let quick = now.saturating_duration_since(self.began) < ms(TAP_MS) && self.moved < TAP_SLOP;
        match self.gesture.clone() {
            Gesture::Point {
                drag: DragState::Dragging,
            } => {
                let at = out.cursor();
                out.button(Button::Left, false, at, 1);
                self.last_tap = None;
            }
            Gesture::Point { drag } if quick && self.most == 1 => {
                let double = drag == DragState::Armed
                    || self
                        .last_click
                        .is_some_and(|t| now.saturating_duration_since(t) < ms(DOUBLE_MS));
                let n = if double { 2 } else { 1 };
                let at = out.cursor();
                out.button(Button::Left, true, at, n);
                out.button(Button::Left, false, at, n);
                self.last_click = Some(now);
                self.last_tap = (!double).then_some(now);
            }
            Gesture::Two if quick && self.most == 2 => {
                let at = out.cursor();
                out.button(Button::Right, true, at, 1);
                out.button(Button::Right, false, at, 1);
                self.last_tap = None;
            }
            Gesture::Scroll => {
                out.scroll((0.0, 0.0), Phase::Ended);
                self.begin_glide(now);
            }
            _ => {
                self.finish(out);
                self.last_tap = None;
            }
        }
        self.gesture = Gesture::Idle;
    }
}

/// A key chord: modifiers held for exactly one key press.
fn chord(out: &mut dyn Poster, kvk: u16, flags: u64) {
    let f = flags | super::keymap::nav_flags(kvk);
    out.key(kvk, true, f);
    out.key(kvk, false, f);
}

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

fn scaled(d: (f64, f64), k: f64) -> (f64, f64) {
    (d.0 * k, d.1 * k)
}

fn centroid(f: &BTreeMap<u8, (f64, f64)>) -> (f64, f64) {
    let n = f.len().max(1) as f64;
    let (x, y) = f.values().fold((0.0, 0.0), |a, p| (a.0 + p.0, a.1 + p.1));
    (x / n, y / n)
}

/// How far apart the fingers are: the mean distance from their centre.
fn spread(f: &BTreeMap<u8, (f64, f64)>) -> f64 {
    let c = centroid(f);
    let n = f.len().max(1) as f64;
    f.values()
        .map(|p| (p.0 - c.0).hypot(p.1 - c.1))
        .sum::<f64>()
        / n
}

/// The fingers' mean motion, over the fingers down in both frames.
fn mean_delta(a: &BTreeMap<u8, (f64, f64)>, b: &BTreeMap<u8, (f64, f64)>) -> (f64, f64) {
    let mut n = 0.0;
    let mut d = (0.0, 0.0);
    for (id, q) in b {
        if let Some(p) = a.get(id) {
            d.0 += q.0 - p.0;
            d.1 += q.1 - p.1;
            n += 1.0;
        }
    }
    if n == 0.0 {
        (0.0, 0.0)
    } else {
        (d.0 / n, d.1 / n)
    }
}

#[cfg(test)]
mod tests {
    use super::super::post::fake::{Ev, Rec};
    use super::*;

    /// A 400 × 300 pad: one CSS px is 65535/400 contact units across.
    fn u(px: f64, across: f64) -> u16 {
        (px * 65535.0 / across).round() as u16
    }
    fn c(id: u8, x: f64, y: f64) -> Contact {
        (id, u(x, 400.0), u(y, 300.0))
    }

    struct T {
        i: Interpreter,
        r: Rec,
        t0: Instant,
    }

    impl T {
        fn new() -> T {
            let t0 = Instant::now();
            T {
                i: Interpreter::new(t0),
                r: Rec::default(),
                t0,
            }
        }
        fn at(&self, ms: u64) -> Instant {
            self.t0 + Duration::from_millis(ms)
        }
        fn f(&mut self, ms: u64, cs: &[Contact]) {
            let now = self.at(ms);
            self.i.frame(now, cs, &mut self.r);
        }
        fn tick(&mut self, ms: u64) {
            let now = self.at(ms);
            self.i.tick(now, &mut self.r);
        }
    }

    #[test]
    fn tap_clicks() {
        let mut t = T::new();
        t.f(0, &[c(1, 100.0, 100.0)]);
        t.f(80, &[]);
        assert_eq!(
            t.r.buttons(),
            [(Button::Left, true, 1), (Button::Left, false, 1)]
        );
    }

    #[test]
    fn a_slow_touch_is_not_a_tap() {
        let mut t = T::new();
        t.f(0, &[c(1, 100.0, 100.0)]);
        t.f(400, &[]);
        assert!(t.r.buttons().is_empty());
    }

    #[test]
    fn one_finger_moves_the_pointer() {
        let mut t = T::new();
        t.f(0, &[c(1, 100.0, 100.0)]);
        t.f(16, &[c(1, 110.0, 100.0)]);
        let moved =
            t.r.evs
                .iter()
                .any(|e| matches!(e, Ev::Move((x, _), None) if *x > 510.0));
        assert!(moved, "{:?}", t.r.evs);
    }

    #[test]
    fn a_burst_after_a_stall_is_not_flung() {
        // Steady motion, one frame that carries three frames' worth (a late arrival), then
        // steady again: the burst moves about as far as the three frames would have.
        let mut t = T::new();
        t.f(0, &[c(1, 100.0, 100.0)]);
        for k in 1..=5u64 {
            t.f(8 * k, &[c(1, 100.0 + 2.0 * k as f64, 100.0)]);
        }
        let before = t.r.at.0;
        t.f(48, &[c(1, 116.0, 100.0)]);
        let jump = t.r.at.0 - before;
        assert!(jump < 6.0 * GAIN * 1.6, "a burst of 6 px went {jump} pt");
    }

    #[test]
    fn two_finger_tap_right_clicks() {
        let mut t = T::new();
        t.f(0, &[c(1, 100.0, 100.0), c(2, 140.0, 100.0)]);
        t.f(90, &[]);
        assert_eq!(
            t.r.buttons(),
            [(Button::Right, true, 1), (Button::Right, false, 1)]
        );
    }

    #[test]
    fn two_finger_tap_lifting_one_at_a_time_right_clicks() {
        let mut t = T::new();
        t.f(0, &[c(1, 100.0, 100.0), c(2, 140.0, 100.0)]);
        t.f(60, &[c(2, 140.0, 100.0)]);
        t.f(90, &[]);
        assert_eq!(
            t.r.buttons(),
            [(Button::Right, true, 1), (Button::Right, false, 1)]
        );
    }

    #[test]
    fn tap_hold_drags_with_dragged_events() {
        let mut t = T::new();
        t.f(0, &[c(1, 100.0, 100.0)]);
        t.f(60, &[]);
        t.f(150, &[c(1, 100.0, 100.0)]);
        t.f(170, &[c(1, 120.0, 100.0)]);
        t.f(190, &[c(1, 140.0, 100.0)]);
        t.f(400, &[]);
        let b = t.r.buttons();
        assert_eq!(b[2], (Button::Left, true, 1), "{b:?}");
        assert_eq!(b.last(), Some(&(Button::Left, false, 1)));
        assert!(
            t.r.evs
                .iter()
                .any(|e| matches!(e, Ev::Move(_, Some(Button::Left)))),
            "{:?}",
            t.r.evs
        );
    }

    #[test]
    fn double_tap_double_clicks() {
        let mut t = T::new();
        t.f(0, &[c(1, 100.0, 100.0)]);
        t.f(60, &[]);
        t.f(150, &[c(1, 100.0, 100.0)]);
        t.f(210, &[]);
        assert_eq!(t.r.buttons().last(), Some(&(Button::Left, false, 2)));
    }

    fn scroll_down(t: &mut T) {
        t.f(0, &[c(1, 100.0, 100.0), c(2, 140.0, 100.0)]);
        for k in 1..=6u64 {
            let y = 100.0 + 15.0 * k as f64;
            t.f(16 * k, &[c(1, 100.0, y), c(2, 140.0, y)]);
        }
        t.f(16 * 7, &[]);
    }

    #[test]
    fn two_finger_scroll_has_began_changed_ended_then_momentum() {
        let mut t = T::new();
        scroll_down(&mut t);
        for k in 8..200u64 {
            t.tick(16 * k);
        }
        let p = t.r.phases();
        assert_eq!(p.first(), Some(&Phase::Began), "{p:?}");
        assert!(p.contains(&Phase::Changed));
        let ended = p.iter().position(|x| *x == Phase::Ended).unwrap();
        assert_eq!(p[ended + 1], Phase::MomentumBegan, "{p:?}");
        assert_eq!(p.last(), Some(&Phase::MomentumEnded));
        assert!(!t.i.gliding());
        // Natural scrolling: fingers down, content down — positive y.
        let first = t.r.evs.iter().find_map(|e| match e {
            Ev::Scroll(d, Phase::Changed) => Some(*d),
            _ => None,
        });
        assert!(first.is_some_and(|d| d.1 > 0.0), "{first:?}");
    }

    #[test]
    fn momentum_and_scroll_phase_never_on_one_event() {
        // Each Phase is one or the other by construction; what must hold is that a new touch
        // ends a glide before anything else is posted.
        let mut t = T::new();
        scroll_down(&mut t);
        t.tick(16 * 8);
        t.tick(16 * 9);
        t.f(16 * 10, &[c(1, 50.0, 50.0)]);
        let p = t.r.phases();
        assert_eq!(p.last(), Some(&Phase::MomentumEnded), "{p:?}");
        assert!(!t.i.gliding());
    }

    fn three(x: f64, y: f64) -> [Contact; 3] {
        [c(1, x, y), c(2, x + 40.0, y), c(3, x + 80.0, y)]
    }

    #[test]
    fn three_finger_up_follows_mission_control() {
        let mut t = T::new();
        for (k, y) in [250.0, 230.0, 200.0, 150.0, 100.0].into_iter().enumerate() {
            t.f(16 * k as u64, &three(100.0, y));
        }
        t.f(100, &[]);
        let d = t.r.docks();
        assert_eq!(d[0].0, Dock::Mission, "{d:?}");
        assert_eq!(d[0].2, Stage::Began);
        assert!(
            d.windows(2).all(|w| w[1].1 <= w[0].1),
            "progress one way: {d:?}"
        );
        let last = d.last().unwrap();
        assert_eq!(last.2, Stage::Ended);
        // 150 px of a 300 px pad: the whole way.
        assert!((last.1 + 1.0).abs() < 0.01, "{last:?}");
        assert!(t.r.keys().is_empty());
    }

    #[test]
    fn four_fingers_left_follow_spaces() {
        let mut t = T::new();
        let four = |x: f64| {
            [
                c(1, x, 150.0),
                c(2, x + 30.0, 150.0),
                c(3, x + 60.0, 150.0),
                c(4, x + 90.0, 150.0),
            ]
        };
        t.f(0, &four(250.0));
        t.f(16, &four(200.0));
        t.f(32, &four(150.0));
        t.f(48, &[]);
        let d = t.r.docks();
        assert_eq!(d[0].0, Dock::Spaces);
        assert!(
            d.last().unwrap().1 > 0.0 && d.last().unwrap().2 == Stage::Ended,
            "{d:?}"
        );
    }

    #[test]
    fn turning_back_cancels() {
        let mut t = T::new();
        t.f(0, &three(150.0, 150.0));
        t.f(16, &three(200.0, 150.0));
        t.f(32, &three(190.0, 150.0));
        t.f(48, &[]);
        assert_eq!(t.r.docks().last().unwrap().2, Stage::Cancelled);
    }

    #[test]
    fn a_swipe_turned_off_does_nothing() {
        let mut t = T::new();
        t.i.prefs.three_v = false;
        t.f(0, &three(100.0, 250.0));
        t.f(16, &three(100.0, 150.0));
        t.f(32, &[]);
        assert!(t.r.evs.is_empty(), "{:?}", t.r.evs);
    }

    #[test]
    fn pages_setting_goes_back() {
        let mut t = T::new();
        t.i.prefs.three_h = Horiz::Pages;
        t.f(0, &three(50.0, 150.0));
        t.f(16, &three(100.0, 150.0));
        t.f(32, &three(160.0, 150.0));
        assert_eq!(t.r.keys()[0], (33, true, flag::CMD));
        assert!(t.r.docks().is_empty());
    }

    #[test]
    fn four_finger_spread_is_show_desktop() {
        let mut t = T::new();
        let four = |r: f64| {
            [
                c(1, 200.0 - r, 150.0),
                c(2, 200.0 + r, 150.0),
                c(3, 200.0, 150.0 - r),
                c(4, 200.0, 150.0 + r),
            ]
        };
        t.f(0, &four(30.0));
        t.f(16, &four(50.0));
        t.f(32, &four(70.0));
        t.f(48, &[]);
        let d = t.r.docks();
        assert_eq!(d[0].0, Dock::Pinch);
        assert!(d.last().unwrap().1 < 0.0, "{d:?}");
    }

    #[test]
    fn pinch_out_zooms_in() {
        let mut t = T::new();
        t.f(0, &[c(1, 180.0, 150.0), c(2, 220.0, 150.0)]);
        t.f(16, &[c(1, 160.0, 150.0), c(2, 240.0, 150.0)]);
        t.f(32, &[c(1, 140.0, 150.0), c(2, 260.0, 150.0)]);
        t.f(48, &[]);
        let z: Vec<_> =
            t.r.evs
                .iter()
                .filter_map(|e| match e {
                    Ev::Magnify(b, s) => Some((*b, *s)),
                    _ => None,
                })
                .collect();
        assert_eq!(z.first().map(|z| z.1), Some(Stage::Began), "{z:?}");
        assert!(z.iter().any(|(b, s)| *s == Stage::Changed && *b > 0.0));
        assert_eq!(z.last().map(|z| z.1), Some(Stage::Ended));
        assert!(t.r.phases().is_empty(), "a pinch does not scroll");
        assert!(t.r.keys().is_empty());
    }
}
