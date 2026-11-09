//! The one file that posts to CoreGraphics: the real [`Poster`].
//!
//! Every event carries its flags explicitly, so a modifier left in the system's state by
//! something else never turns a click into a ⌘-click; drags are posted as `…MouseDragged`, and
//! scroll events carry the trackpad's phases so apps glide and rubber-band as they do natively.

use objc2_core_foundation::{CFRetained, CGPoint, CGRect};
use objc2_core_graphics::{
    CGDirectDisplayID, CGDisplayBounds, CGError, CGEvent, CGEventField, CGEventFlags,
    CGEventSource, CGEventSourceStateID, CGEventTapLocation, CGEventType, CGGetActiveDisplayList,
    CGMouseButton, CGScrollEventUnit,
};

use super::post::{Button, Dock, Phase, Poster, Stage};

const HID_TAP: CGEventTapLocation = CGEventTapLocation::HIDEventTap;

// Gesture events, as a trackpad's reach the Dock and apps. These fields are undocumented; the
// numbers are the ones Mac Mouse Fix found (`TouchSimulator.m`, its pre-macOS-27 path).
const SESSION_TAP: CGEventTapLocation = CGEventTapLocation::SessionEventTap;
const F_TYPE: CGEventField = CGEventField(55);
const F_41: CGEventField = CGEventField(41);
const V_41: f64 = 33231.0;
const F_SUBTYPE: CGEventField = CGEventField(110);
const F_ZOOM: CGEventField = CGEventField(113);
const F_MOTION_BITS: CGEventField = CGEventField(119);
const F_MOTION: CGEventField = CGEventField(123);
const F_PROGRESS: CGEventField = CGEventField(124);
const F_EXIT_X: CGEventField = CGEventField(129);
const F_EXIT_Y: CGEventField = CGEventField(130);
const F_PHASE: CGEventField = CGEventField(132);
const F_PHASE_2: CGEventField = CGEventField(134);
const F_PROGRESS_BITS: CGEventField = CGEventField(135);
const F_INVERTED: CGEventField = CGEventField(136);
const F_MOTION_BITS_2: CGEventField = CGEventField(139);
const F_MOTION_2: CGEventField = CGEventField(165);
const T_GESTURE: i64 = 29;
const T_DOCK: i64 = 30;
const SUB_ZOOM: i64 = 8;
const SUB_DOCK_SWIPE: i64 = 23;

/// `IOHIDEventPhaseBits`.
fn phase_bits(s: Stage) -> i64 {
    match s {
        Stage::Began => 1,
        Stage::Changed => 2,
        Stage::Ended => 4,
        Stage::Cancelled => 8,
    }
}

/// Bumped by every Dock gesture that begins, so a late repeat of an end never ends a newer one.
static DOCK_GEN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Post one Dock swipe: a type-30 event with the motion, followed by a bare gesture event.
fn post_dock(kind: Dock, progress: f64, stage: Stage, exit: f64) {
    let (phase, motion) = (phase_bits(stage) as f64, kind as i64);
    let (Some(e30), Some(e29)) = (CGEvent::new(None), CGEvent::new(None)) else {
        return;
    };
    let (e30, e29) = (Some(&*e30), Some(&*e29));
    CGEvent::set_double_value_field(e30, F_TYPE, T_DOCK as f64);
    CGEvent::set_double_value_field(e30, F_SUBTYPE, SUB_DOCK_SWIPE as f64);
    CGEvent::set_double_value_field(e30, F_PHASE, phase);
    CGEvent::set_double_value_field(e30, F_PHASE_2, phase);
    CGEvent::set_double_value_field(e30, F_PROGRESS, progress);
    let progress_bits = i64::from((progress as f32).to_bits());
    CGEvent::set_integer_value_field(e30, F_PROGRESS_BITS, progress_bits);
    CGEvent::set_double_value_field(e30, F_41, V_41);
    // The motion again, as the bits of a float.
    let bits = f64::from(f32::from_bits(motion as u32));
    CGEvent::set_double_value_field(e30, F_MOTION_BITS, bits);
    CGEvent::set_double_value_field(e30, F_MOTION_BITS_2, bits);
    CGEvent::set_double_value_field(e30, F_MOTION, motion as f64);
    CGEvent::set_double_value_field(e30, F_MOTION_2, motion as f64);
    // Progress is given as natural scrolling reads it (content follows the fingers).
    CGEvent::set_integer_value_field(e30, F_INVERTED, 1);
    if matches!(stage, Stage::Ended | Stage::Cancelled) {
        CGEvent::set_double_value_field(e30, F_EXIT_X, exit);
        CGEvent::set_double_value_field(e30, F_EXIT_Y, exit);
    }
    CGEvent::set_double_value_field(e29, F_TYPE, T_GESTURE as f64);
    CGEvent::set_double_value_field(e29, F_41, V_41);
    CGEvent::post(SESSION_TAP, e30);
    CGEvent::post(SESSION_TAP, e29);
}

/// The modifiers a phone holds: pointer events carry exactly these, never a stray one.
fn held_flags() -> CGEventFlags {
    CGEventFlags(super::HELD.load(std::sync::atomic::Ordering::Relaxed))
}

/// Most UTF-16 units one event carries; longer text is split.
pub const TEXT_CHUNK: usize = 20;

/// Posts through a HID-level event source (none, if macOS gives none: CG takes that too).
pub struct CgPoster {
    source: Option<CFRetained<CGEventSource>>,
}

// SAFETY: CGEventSource is a CF object usable from any thread; one owner posts through it.
unsafe impl Send for CgPoster {}

impl CgPoster {
    pub fn new() -> CgPoster {
        CgPoster {
            source: CGEventSource::new(CGEventSourceStateID::HIDSystemState),
        }
    }
}

impl Default for CgPoster {
    fn default() -> Self {
        Self::new()
    }
}

/// The active displays' ids.
pub fn display_ids() -> impl Iterator<Item = CGDirectDisplayID> + Clone {
    let mut ids = [0; 16];
    let mut n = 0;
    // SAFETY: room for 16 ids, and the count written back.
    let ok = unsafe { CGGetActiveDisplayList(16, ids.as_mut_ptr(), &mut n) } == CGError::Success;
    ids.into_iter().take(if ok { n as usize } else { 0 })
}

fn inside(r: &CGRect, p: (f64, f64)) -> bool {
    p.0 >= r.origin.x
        && p.0 < r.origin.x + r.size.width
        && p.1 >= r.origin.y
        && p.1 < r.origin.y + r.size.height
}

impl Poster for CgPoster {
    fn cursor(&mut self) -> (f64, f64) {
        // An empty event reports the pointer's location.
        CGEvent::new(None).map_or((0.0, 0.0), |e| {
            let p = CGEvent::location(Some(&e));
            (p.x, p.y)
        })
    }

    fn clamp(&mut self, p: (f64, f64), prev: (f64, f64)) -> (f64, f64) {
        let ds = display_ids().map(|id| CGDisplayBounds(id));
        if ds.clone().any(|r| inside(&r, p)) {
            return p;
        }
        // Off every screen: hold it at the edge of the screen it was on.
        let Some(r) = ds.clone().find(|r| inside(r, prev)).or(ds.clone().next()) else {
            return prev;
        };
        (
            p.0.clamp(r.origin.x, r.origin.x + r.size.width - 1.0),
            p.1.clamp(r.origin.y, r.origin.y + r.size.height - 1.0),
        )
    }

    fn move_to(&mut self, to: (f64, f64), d: (f64, f64), held: Option<Button>) {
        let kind = match held {
            None => CGEventType::MouseMoved,
            Some(Button::Left) => CGEventType::LeftMouseDragged,
            Some(Button::Right) => CGEventType::RightMouseDragged,
        };
        let button = CGMouseButton(u32::from(held == Some(Button::Right)));
        let at = CGPoint::new(to.0, to.1);
        let Some(e) = CGEvent::new_mouse_event(self.source.as_deref(), kind, at, button) else {
            return;
        };
        let e = Some(&*e);
        CGEvent::set_integer_value_field(e, CGEventField::MouseEventDeltaX, d.0.round() as i64);
        CGEvent::set_integer_value_field(e, CGEventField::MouseEventDeltaY, d.1.round() as i64);
        CGEvent::set_flags(e, held_flags());
        CGEvent::post(HID_TAP, e);
    }

    fn button(&mut self, b: Button, down: bool, at: (f64, f64), clicks: i64) {
        let kind = match (b, down) {
            (Button::Left, true) => CGEventType::LeftMouseDown,
            (Button::Left, false) => CGEventType::LeftMouseUp,
            (Button::Right, true) => CGEventType::RightMouseDown,
            (Button::Right, false) => CGEventType::RightMouseUp,
        };
        let button = CGMouseButton(u32::from(b == Button::Right));
        let at = CGPoint::new(at.0, at.1);
        let Some(e) = CGEvent::new_mouse_event(self.source.as_deref(), kind, at, button) else {
            return;
        };
        let e = Some(&*e);
        CGEvent::set_integer_value_field(e, CGEventField::MouseEventClickState, clicks);
        CGEvent::set_flags(e, held_flags());
        CGEvent::post(HID_TAP, e);
    }

    fn scroll(&mut self, d: (f64, f64), phase: Phase) {
        let at = self.cursor();
        let Some(e) = CGEvent::new_scroll_wheel_event2(
            self.source.as_deref(),
            CGScrollEventUnit::Pixel,
            2,
            d.1 as i32,
            d.0 as i32,
            0,
        ) else {
            return;
        };
        let e = Some(&*e);
        CGEvent::set_integer_value_field(e, CGEventField::ScrollWheelEventIsContinuous, 1);
        CGEvent::set_double_value_field(e, CGEventField::ScrollWheelEventPointDeltaAxis1, d.1);
        CGEvent::set_double_value_field(e, CGEventField::ScrollWheelEventPointDeltaAxis2, d.0);
        let (scroll, momentum) = match phase {
            Phase::Began => (1, 0),
            Phase::Changed => (2, 0),
            Phase::Ended => (4, 0),
            Phase::MomentumBegan => (0, 1),
            Phase::Momentum => (0, 2),
            Phase::MomentumEnded => (0, 3),
        };
        CGEvent::set_integer_value_field(e, CGEventField::ScrollWheelEventScrollPhase, scroll);
        CGEvent::set_integer_value_field(e, CGEventField::ScrollWheelEventMomentumPhase, momentum);
        CGEvent::set_location(e, CGPoint::new(at.0, at.1));
        CGEvent::set_flags(e, held_flags());
        CGEvent::post(HID_TAP, e);
    }

    fn key(&mut self, kvk: u16, down: bool, flags: u64) {
        let Some(e) = CGEvent::new_keyboard_event(self.source.as_deref(), kvk, down) else {
            return;
        };
        CGEvent::set_flags(Some(&e), CGEventFlags(flags));
        CGEvent::post(HID_TAP, Some(&e));
    }

    fn dock(&mut self, kind: Dock, progress: f64, stage: Stage, exit: f64) {
        use std::sync::atomic::Ordering::Relaxed;
        if stage == Stage::Began {
            DOCK_GEN.fetch_add(1, Relaxed);
        }
        tracing::debug!(?kind, progress, ?stage, "pad: dock");
        post_dock(kind, progress, stage, exit);
        // A busy Dock sometimes misses the end and stays mid-transition; sent again 200 ms
        // later, as Mac Mouse Fix does, unless another gesture has begun since.
        if matches!(stage, Stage::Ended | Stage::Cancelled) {
            let generation = DOCK_GEN.load(Relaxed);
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(200));
                if DOCK_GEN.load(Relaxed) == generation {
                    post_dock(kind, progress, stage, exit);
                }
            });
        }
    }

    fn magnify(&mut self, by: f64, stage: Stage) {
        let at = self.cursor();
        let Some(e) = CGEvent::new(None) else {
            return;
        };
        let e = Some(&*e);
        CGEvent::set_integer_value_field(e, F_TYPE, T_GESTURE);
        CGEvent::set_integer_value_field(e, F_SUBTYPE, SUB_ZOOM);
        CGEvent::set_integer_value_field(e, F_PHASE, phase_bits(stage));
        CGEvent::set_double_value_field(e, F_ZOOM, by);
        CGEvent::set_location(e, CGPoint::new(at.0, at.1));
        CGEvent::post(SESSION_TAP, e);
    }

    fn text(&mut self, s: &str) {
        let units: Vec<u16> = s.encode_utf16().collect();
        for chunk in super::text_chunks(&units, TEXT_CHUNK) {
            for down in [true, false] {
                // Key 0 carrying `chunk` as its text; flags cleared so a held ⌘ from elsewhere
                // cannot turn the text into shortcuts.
                let Some(e) = CGEvent::new_keyboard_event(self.source.as_deref(), 0, down) else {
                    return;
                };
                let e = Some(&*e);
                // SAFETY: `chunk` is `chunk.len()` UTF-16 units, alive for the call.
                unsafe {
                    CGEvent::keyboard_set_unicode_string(e, chunk.len() as _, chunk.as_ptr())
                };
                CGEvent::set_flags(e, CGEventFlags(0));
                CGEvent::post(HID_TAP, e);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Read-only: nothing is posted.
    #[test]
    fn a_point_off_every_screen_is_held_on_one() {
        let mut p = CgPoster::new();
        let at = p.cursor();
        if display_ids().next().is_none() {
            return eprintln!("no display here");
        }
        assert_eq!(p.clamp(at, at), at, "the pointer is on a screen");
        let held = p.clamp((-1e7, -1e7), at);
        assert!(
            display_ids().any(|id| inside(&CGDisplayBounds(id), held)),
            "{held:?}"
        );
    }
}
