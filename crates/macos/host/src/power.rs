//! Keeping the Mac awake (an IOKit power assertion, released on drop), and locking it or
//! putting it to sleep.

use std::sync::Mutex;
use std::time::Duration;

use objc2_core_foundation::CFString;
use tokio::sync::watch;
use wado_platform::power::{PowerInhibit, PowerOp, SystemPower};
use wado_platform::{Fut, Res};

#[link(name = "IOKit", kind = "framework")]
unsafe extern "C" {
    fn IOPMAssertionCreateWithName(
        kind: *const CFString,
        level: u32,
        name: *const CFString,
        id: *mut u32,
    ) -> i32;
    fn IOPMAssertionRelease(id: u32) -> i32;
}

const ON: u32 = 255; // kIOPMAssertionLevelOn

/// One "keep awake" assertion while on.
pub struct MacAwake {
    held: Mutex<Option<u32>>,
    tx: watch::Sender<bool>,
}

impl MacAwake {
    pub fn new() -> MacAwake {
        MacAwake {
            held: Mutex::new(None),
            tx: watch::channel(false).0,
        }
    }
}

impl Default for MacAwake {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for MacAwake {
    fn drop(&mut self) {
        let _ = self.set(false);
    }
}

impl PowerInhibit for MacAwake {
    fn state(&self) -> watch::Receiver<bool> {
        self.tx.subscribe()
    }

    fn set(&self, on: bool) -> Res<()> {
        let mut held = self.held.lock().unwrap_or_else(|e| e.into_inner());
        match (on, *held) {
            (true, None) => {
                let kind = CFString::from_str("PreventUserIdleSystemSleep");
                let name = CFString::from_str("wado: a phone asked this Mac to stay awake");
                let mut id = 0u32;
                // SAFETY: two live CFStrings and an out-pointer for the id.
                let r = unsafe { IOPMAssertionCreateWithName(&*kind, ON, &*name, &mut id) };
                if r != 0 {
                    return Err(format!("macOS refused to keep the Mac awake ({r:#x})"));
                }
                *held = Some(id);
            }
            (false, Some(id)) => {
                // SAFETY: an id this process created and has not released.
                unsafe { IOPMAssertionRelease(id) };
                *held = None;
            }
            _ => {}
        }
        self.tx.send_replace(on);
        Ok(())
    }
}

/// Lock the screen, or sleep.
pub struct MacPower;

const LOCK_LIB: &str = "/System/Library/PrivateFrameworks/login.framework/Versions/Current/login";

/// The screen-lock call the menu bar's own "Lock Screen" uses, looked up at run time — it is
/// private, so a Mac without it falls back to sleeping the display, which locks when the Mac
/// asks for a password on wake.
fn lock_now() -> Result<(), String> {
    let lib = std::ffi::CString::new(LOCK_LIB).unwrap();
    let sym = std::ffi::CString::new("SACLockScreenImmediate").unwrap();
    // SAFETY: dlopen/dlsym with NUL-terminated names; the symbol, when present, takes no
    // arguments and returns an int.
    unsafe {
        let h = libc::dlopen(lib.as_ptr(), libc::RTLD_LAZY);
        if !h.is_null() {
            let f = libc::dlsym(h, sym.as_ptr());
            if !f.is_null() {
                let f: extern "C" fn() -> i32 = std::mem::transmute(f);
                f();
                return Ok(());
            }
        }
    }
    crate::cmd::detached("/usr/bin/pmset", &["displaysleepnow"])
}

impl SystemPower for MacPower {
    fn can(&self, _op: PowerOp) -> Res<()> {
        Ok(())
    }

    fn run(&self, op: PowerOp) -> Fut<'_, Res<()>> {
        Box::pin(async move {
            match op {
                PowerOp::Lock => lock_now(),
                PowerOp::Suspend => {
                    crate::cmd::run("/usr/bin/pmset", &["sleepnow"], Duration::from_secs(5))
                        .await
                        .map(drop)
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn power_inhibit_toggles_state() {
        let a = MacAwake::new();
        let rx = a.state();
        a.set(true).unwrap();
        assert!(*rx.borrow());
        assert!(a.held.lock().unwrap().is_some(), "an assertion is held");
        a.set(true).unwrap();
        a.set(false).unwrap();
        assert!(!*rx.borrow());
        assert!(a.held.lock().unwrap().is_none(), "and released");
    }
}
