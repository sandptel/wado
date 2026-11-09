//! The one file that posts to CoreGraphics: the real [`Poster`].
//!
//! Every event carries its flags explicitly, so a modifier left in the system's state by
//! something else never turns a click into a ⌘-click; drags are posted as `…MouseDragged`, and
//! scroll events carry the trackpad's phases so apps glide and rubber-band as they do natively.

use std::ffi::c_void;

use super::post::{Button, Phase, Poster};

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
pub fn post_event(e: Ref) {
    if !e.is_null() {
        // SAFETY: a live event the caller keeps alive for this call.
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
