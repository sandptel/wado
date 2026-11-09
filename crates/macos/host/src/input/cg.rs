//! The one file that posts to CoreGraphics: the real [`Poster`].
//!
//! Every event carries its flags explicitly, so a modifier left in the system's state by
//! something else never turns a click into a ⌘-click; drags are posted as `…MouseDragged`, and
//! scroll events carry the trackpad's phases so apps glide and rubber-band as they do natively.

use std::ffi::c_void;

use super::post::{Button, Dock, Phase, Poster, Stage};

type Ref = *mut c_void;

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct CGPoint {
    pub x: f64,
    pub y: f64,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct CGSize {
    pub width: f64,
    pub height: f64,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct CGRect {
    pub origin: CGPoint,
    pub size: CGSize,
}

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGEventSourceCreate(state: i32) -> Ref;
    fn CGEventCreate(source: Ref) -> Ref;
    fn CGEventGetLocation(e: Ref) -> CGPoint;
    fn CGEventCreateMouseEvent(source: Ref, kind: u32, at: CGPoint, button: u32) -> Ref;
    fn CGEventCreateKeyboardEvent(source: Ref, key: u16, down: bool) -> Ref;
    fn CGEventCreateScrollWheelEvent2(
        source: Ref,
        units: u32,
        count: u32,
        w1: i32,
        w2: i32,
        w3: i32,
    ) -> Ref;
    fn CGEventSetIntegerValueField(e: Ref, field: u32, v: i64);
    fn CGEventSetDoubleValueField(e: Ref, field: u32, v: f64);
    fn CGEventSetFlags(e: Ref, flags: u64);
    fn CGEventSetLocation(e: Ref, at: CGPoint);
    fn CGEventKeyboardSetUnicodeString(e: Ref, len: usize, s: *const u16);
    fn CGEventPost(tap: u32, e: Ref);
    fn CGGetActiveDisplayList(max: u32, ids: *mut u32, count: *mut u32) -> i32;
    fn CGDisplayBounds(id: u32) -> CGRect;
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFRelease(r: Ref);
}

const HID_SYSTEM_STATE: i32 = 1;
const HID_TAP: u32 = 0;
const UNIT_PIXEL: u32 = 0;

// CGEventType
const LEFT_DOWN: u32 = 1;
const LEFT_UP: u32 = 2;
const RIGHT_DOWN: u32 = 3;
const RIGHT_UP: u32 = 4;
const MOVED: u32 = 5;
const LEFT_DRAGGED: u32 = 6;
const RIGHT_DRAGGED: u32 = 7;

// CGEventField
const CLICK_STATE: u32 = 1;
const DELTA_X: u32 = 4;
const DELTA_Y: u32 = 5;
const IS_CONTINUOUS: u32 = 88;
const POINT_DELTA_1: u32 = 96;
const POINT_DELTA_2: u32 = 97;
const SCROLL_PHASE: u32 = 99;
const MOMENTUM_PHASE: u32 = 123;

// Gesture events, as a trackpad's reach the Dock and apps. These fields are undocumented; the
// numbers are the ones Mac Mouse Fix found (`TouchSimulator.m`, its pre-macOS-27 path).
const SESSION_TAP: u32 = 1;
const F_TYPE: u32 = 55;
const F_41: u32 = 41;
const V_41: f64 = 33231.0;
const F_SUBTYPE: u32 = 110;
const F_ZOOM: u32 = 113;
const F_MOTION_BITS: u32 = 119;
const F_MOTION: u32 = 123;
const F_PROGRESS: u32 = 124;
const F_EXIT_X: u32 = 129;
const F_EXIT_Y: u32 = 130;
const F_PHASE: u32 = 132;
const F_PHASE_2: u32 = 134;
const F_PROGRESS_BITS: u32 = 135;
const F_INVERTED: u32 = 136;
const F_MOTION_BITS_2: u32 = 139;
const F_MOTION_2: u32 = 165;
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
    // SAFETY: events we create, fill, post and release once each.
    unsafe {
        let e30 = CGEventCreate(std::ptr::null_mut());
        let e29 = CGEventCreate(std::ptr::null_mut());
        if e30.is_null() || e29.is_null() {
            return;
        }
        CGEventSetDoubleValueField(e30, F_TYPE, T_DOCK as f64);
        CGEventSetDoubleValueField(e30, F_SUBTYPE, SUB_DOCK_SWIPE as f64);
        CGEventSetDoubleValueField(e30, F_PHASE, phase);
        CGEventSetDoubleValueField(e30, F_PHASE_2, phase);
        CGEventSetDoubleValueField(e30, F_PROGRESS, progress);
        CGEventSetIntegerValueField(e30, F_PROGRESS_BITS, i64::from((progress as f32).to_bits()));
        CGEventSetDoubleValueField(e30, F_41, V_41);
        // The motion again, as the bits of a float.
        let bits = f64::from(f32::from_bits(motion as u32));
        CGEventSetDoubleValueField(e30, F_MOTION_BITS, bits);
        CGEventSetDoubleValueField(e30, F_MOTION_BITS_2, bits);
        CGEventSetDoubleValueField(e30, F_MOTION, motion as f64);
        CGEventSetDoubleValueField(e30, F_MOTION_2, motion as f64);
        // Progress is given as natural scrolling reads it (content follows the fingers).
        CGEventSetIntegerValueField(e30, F_INVERTED, 1);
        if matches!(stage, Stage::Ended | Stage::Cancelled) {
            CGEventSetDoubleValueField(e30, F_EXIT_X, exit);
            CGEventSetDoubleValueField(e30, F_EXIT_Y, exit);
        }
        CGEventSetDoubleValueField(e29, F_TYPE, T_GESTURE as f64);
        CGEventSetDoubleValueField(e29, F_41, V_41);
        CGEventPost(SESSION_TAP, e30);
        CGEventPost(SESSION_TAP, e29);
        CFRelease(e30);
        CFRelease(e29);
    }
}

/// The modifiers a phone holds: pointer events carry exactly these, never a stray one.
fn held_flags() -> u64 {
    super::HELD.load(std::sync::atomic::Ordering::Relaxed)
}

/// Most UTF-16 units one event carries; longer text is split.
pub const TEXT_CHUNK: usize = 20;

/// Posts through a HID-level event source.
pub struct CgPoster {
    source: SendRef,
}

struct SendRef(Ref);
// SAFETY: CGEventSource is a CF object usable from any thread; one owner posts through it.
unsafe impl Send for SendRef {}

impl CgPoster {
    pub fn new() -> CgPoster {
        // SAFETY: creating a source has no preconditions; null is handled by CG as "no source".
        CgPoster {
            source: SendRef(unsafe { CGEventSourceCreate(HID_SYSTEM_STATE) }),
        }
    }

    fn post(&self, e: Ref) {
        if e.is_null() {
            return;
        }
        // SAFETY: a live event we created; posted, then released once.
        unsafe {
            CGEventPost(HID_TAP, e);
            CFRelease(e);
        }
    }
}

impl Default for CgPoster {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for CgPoster {
    fn drop(&mut self) {
        if !self.source.0.is_null() {
            // SAFETY: the source this poster created, released once.
            unsafe { CFRelease(self.source.0) };
        }
    }
}

/// Post an event someone else made (and still owns) at the HID level.
///
/// # Safety
/// `e` is null or a live `CGEventRef` the caller keeps alive for this call.
pub unsafe fn post_event(e: Ref) {
    if !e.is_null() {
        // SAFETY: the caller's promise above.
        unsafe { CGEventPost(HID_TAP, e) };
    }
}

/// The displays' bounds, in global points.
pub fn displays() -> Vec<CGRect> {
    let mut ids = [0u32; 16];
    let mut n = 0u32;
    // SAFETY: room for 16 ids, and the count written back.
    if unsafe { CGGetActiveDisplayList(16, ids.as_mut_ptr(), &mut n) } != 0 {
        return Vec::new();
    }
    ids[..n as usize]
        .iter()
        // SAFETY: an id CoreGraphics just listed.
        .map(|&id| unsafe { CGDisplayBounds(id) })
        .collect()
}

fn inside(r: &CGRect, p: (f64, f64)) -> bool {
    p.0 >= r.origin.x
        && p.0 < r.origin.x + r.size.width
        && p.1 >= r.origin.y
        && p.1 < r.origin.y + r.size.height
}

impl Poster for CgPoster {
    fn cursor(&mut self) -> (f64, f64) {
        // SAFETY: an empty event reports the pointer's location; released once.
        unsafe {
            let e = CGEventCreate(std::ptr::null_mut());
            if e.is_null() {
                return (0.0, 0.0);
            }
            let p = CGEventGetLocation(e);
            CFRelease(e);
            (p.x, p.y)
        }
    }

    fn clamp(&mut self, p: (f64, f64), prev: (f64, f64)) -> (f64, f64) {
        let ds = displays();
        if ds.iter().any(|r| inside(r, p)) {
            return p;
        }
        // Off every screen: hold it at the edge of the screen it was on.
        let Some(r) = ds.iter().find(|r| inside(r, prev)).or(ds.first()) else {
            return prev;
        };
        (
            p.0.clamp(r.origin.x, r.origin.x + r.size.width - 1.0),
            p.1.clamp(r.origin.y, r.origin.y + r.size.height - 1.0),
        )
    }

    fn move_to(&mut self, to: (f64, f64), d: (f64, f64), held: Option<Button>) {
        let kind = match held {
            None => MOVED,
            Some(Button::Left) => LEFT_DRAGGED,
            Some(Button::Right) => RIGHT_DRAGGED,
        };
        let button = u32::from(held == Some(Button::Right));
        // SAFETY: a new event from our source, filled and posted once.
        unsafe {
            let e =
                CGEventCreateMouseEvent(self.source.0, kind, CGPoint { x: to.0, y: to.1 }, button);
            if e.is_null() {
                return;
            }
            CGEventSetIntegerValueField(e, DELTA_X, d.0.round() as i64);
            CGEventSetIntegerValueField(e, DELTA_Y, d.1.round() as i64);
            CGEventSetFlags(e, held_flags());
            self.post(e);
        }
    }

    fn button(&mut self, b: Button, down: bool, at: (f64, f64), clicks: i64) {
        let kind = match (b, down) {
            (Button::Left, true) => LEFT_DOWN,
            (Button::Left, false) => LEFT_UP,
            (Button::Right, true) => RIGHT_DOWN,
            (Button::Right, false) => RIGHT_UP,
        };
        // SAFETY: as above.
        unsafe {
            let e = CGEventCreateMouseEvent(
                self.source.0,
                kind,
                CGPoint { x: at.0, y: at.1 },
                u32::from(b == Button::Right),
            );
            if e.is_null() {
                return;
            }
            CGEventSetIntegerValueField(e, CLICK_STATE, clicks);
            CGEventSetFlags(e, held_flags());
            self.post(e);
        }
    }

    fn scroll(&mut self, d: (f64, f64), phase: Phase) {
        let at = self.cursor();
        // SAFETY: as above.
        unsafe {
            let e = CGEventCreateScrollWheelEvent2(
                self.source.0,
                UNIT_PIXEL,
                2,
                d.1 as i32,
                d.0 as i32,
                0,
            );
            if e.is_null() {
                return;
            }
            CGEventSetIntegerValueField(e, IS_CONTINUOUS, 1);
            CGEventSetDoubleValueField(e, POINT_DELTA_1, d.1);
            CGEventSetDoubleValueField(e, POINT_DELTA_2, d.0);
            let (scroll, momentum) = match phase {
                Phase::Began => (1, 0),
                Phase::Changed => (2, 0),
                Phase::Ended => (4, 0),
                Phase::MomentumBegan => (0, 1),
                Phase::Momentum => (0, 2),
                Phase::MomentumEnded => (0, 3),
            };
            CGEventSetIntegerValueField(e, SCROLL_PHASE, scroll);
            CGEventSetIntegerValueField(e, MOMENTUM_PHASE, momentum);
            CGEventSetLocation(e, CGPoint { x: at.0, y: at.1 });
            CGEventSetFlags(e, held_flags());
            self.post(e);
        }
    }

    fn key(&mut self, kvk: u16, down: bool, flags: u64) {
        // SAFETY: as above.
        unsafe {
            let e = CGEventCreateKeyboardEvent(self.source.0, kvk, down);
            if e.is_null() {
                return;
            }
            CGEventSetFlags(e, flags);
            self.post(e);
        }
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
        // SAFETY: an event we create, fill, post and release once.
        unsafe {
            let e = CGEventCreate(std::ptr::null_mut());
            if e.is_null() {
                return;
            }
            CGEventSetIntegerValueField(e, F_TYPE, T_GESTURE);
            CGEventSetIntegerValueField(e, F_SUBTYPE, SUB_ZOOM);
            CGEventSetIntegerValueField(e, F_PHASE, phase_bits(stage));
            CGEventSetDoubleValueField(e, F_ZOOM, by);
            CGEventSetLocation(
                e,
                CGPoint {
                    x: self.cursor().0,
                    y: self.cursor().1,
                },
            );
            CGEventPost(SESSION_TAP, e);
            CFRelease(e);
        }
    }

    fn text(&mut self, s: &str) {
        let units: Vec<u16> = s.encode_utf16().collect();
        for chunk in super::text_chunks(&units, TEXT_CHUNK) {
            for down in [true, false] {
                // SAFETY: key 0 carrying `chunk` as its text; flags cleared so a held ⌘ from
                // elsewhere cannot turn the text into shortcuts.
                unsafe {
                    let e = CGEventCreateKeyboardEvent(self.source.0, 0, down);
                    if e.is_null() {
                        return;
                    }
                    CGEventKeyboardSetUnicodeString(e, chunk.len(), chunk.as_ptr());
                    CGEventSetFlags(e, 0);
                    self.post(e);
                }
            }
        }
    }
}
