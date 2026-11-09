//! The built-in display's brightness, through DisplayServices — the private framework the
//! brightness keys and Control Centre use. Looked up at run time; a Mac without it, or without
//! a built-in display, has no brightness here.

use std::time::Duration;

use tokio::sync::watch;
use wado_platform::display::Brightness;
use wado_platform::state::Published;
use wado_platform::{Fut, Res};

const LIB: &std::ffi::CStr =
    c"/System/Library/PrivateFrameworks/DisplayServices.framework/DisplayServices";

type Get = unsafe extern "C" fn(u32, *mut f32) -> i32;
type Set = unsafe extern "C" fn(u32, f32) -> i32;

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGGetActiveDisplayList(max: u32, ids: *mut u32, count: *mut u32) -> i32;
    fn CGDisplayIsBuiltin(id: u32) -> bool;
}

/// One brightness step, as the keys take it.
pub const STEP: f32 = 1.0 / 16.0;

pub struct MacBrightness {
    display: u32,
    get: Get,
    set: Set,
    state: Published<Option<f32>>,
}

/// The built-in display, when there is one.
fn builtin() -> Option<u32> {
    let mut ids = [0u32; 16];
    let mut n = 0u32;
    // SAFETY: room for 16 ids, and the count written back.
    if unsafe { CGGetActiveDisplayList(16, ids.as_mut_ptr(), &mut n) } != 0 {
        return None;
    }
    // SAFETY: ids CoreGraphics just listed.
    ids[..n as usize]
        .iter()
        .copied()
        .find(|&id| unsafe { CGDisplayIsBuiltin(id) })
}

impl MacBrightness {
    pub fn probe() -> Result<MacBrightness, String> {
        let display = builtin()
            .ok_or("Not on macOS for external displays yet — this Mac has no built-in display.")?;
        // SAFETY: dlopen/dlsym with NUL-terminated names; the symbols, when present, have these
        // signatures on every macOS that ships them.
        let (get, set) = unsafe {
            let h = libc::dlopen(LIB.as_ptr(), libc::RTLD_LAZY);
            if h.is_null() {
                return Err("this Mac's brightness control is not reachable".into());
            }
            let g = libc::dlsym(h, c"DisplayServicesGetBrightness".as_ptr());
            let s = libc::dlsym(h, c"DisplayServicesSetBrightness".as_ptr());
            if g.is_null() || s.is_null() {
                return Err("this Mac's brightness control is not reachable".into());
            }
            (
                std::mem::transmute::<*mut libc::c_void, Get>(g),
                std::mem::transmute::<*mut libc::c_void, Set>(s),
            )
        };
        let b = MacBrightness {
            display,
            get,
            set,
            state: Published::new(None),
        };
        let now = b.read().ok_or("this Mac does not report its brightness")?;
        b.state.publish(Some(now));
        Ok(b)
    }

    fn read(&self) -> Option<f32> {
        let mut v = 0f32;
        // SAFETY: a display id and an out-pointer, as the call takes.
        (unsafe { (self.get)(self.display, &mut v) } == 0).then_some(v)
    }

    fn write(&self, level: f32) -> Res<()> {
        // SAFETY: a display id and a level in 0..1.
        if unsafe { (self.set)(self.display, level) } != 0 {
            return Err("macOS refused the brightness".into());
        }
        self.state.publish(self.read());
        Ok(())
    }
}

impl Brightness for MacBrightness {
    fn state(&self) -> watch::Receiver<Option<f32>> {
        self.state.subscribe()
    }

    fn fresh(&self, within: Duration) -> Fut<'_, ()> {
        Box::pin(self.state.fresh(within, || async { self.read() }))
    }

    /// Never fully off: a black panel nobody can see to undo is not a brightness.
    fn set(&self, level: f32) -> Fut<'_, Res<()>> {
        Box::pin(async move {
            if !level.is_finite() {
                return Err("that is not a brightness".into());
            }
            self.write(level.clamp(0.01, 1.0))
        })
    }

    fn step(&self, up: bool) -> Fut<'_, Res<()>> {
        Box::pin(async move {
            let now = self
                .read()
                .ok_or("this Mac does not report its brightness")?;
            self.write((now + if up { STEP } else { -STEP }).clamp(0.01, 1.0))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brightness_read_in_range() {
        match MacBrightness::probe() {
            Ok(b) => {
                let v = b.read().unwrap();
                assert!((0.0..=1.0).contains(&v), "{v}");
            }
            Err(why) => eprintln!("no brightness here: {why}"),
        }
    }
}
