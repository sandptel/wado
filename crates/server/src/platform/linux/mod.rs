//! The Linux providers: today's host code, behind the platform traits (`wado_platform`).
//!
//! Each capability is offered only when opening it works here — **opening is the probe**
//! (`CLAUDE.md` invariant 6): `/dev/uinput` opens for writing, the tool is installed, the desktop
//! display was captured. What is missing says why, in the words the person used to be shown when
//! the action failed.
//!
//! Files, one job each:
//! - `uinput.rs` — the raw `/dev/uinput` device: open, describe, emit batches, destroy on drop.
//! - `touchpad.rs` — the virtual touchpad (`TouchInjector`) and its slot logic.
//! - `keyboard.rs` — the virtual keyboard (`KeyInjector`).
//! - `wtype.rs` — typing committed text (`TextInjector`).

pub mod keyboard;
pub mod touchpad;
pub mod uinput;
pub mod wtype;

use std::fs::OpenOptions;
use std::io;
use std::os::unix::fs::OpenOptionsExt;
use std::sync::Arc;

use wado_platform::input::{
    GamepadInjector, InputError, KeyInjector, Opener, TextInjector, TouchInjector,
};
use wado_platform::registry::Offer;
use wado_platform::{Provider, ProviderId};

/// This provider's id.
pub const LINUX: ProviderId = ProviderId("linux");

/// Shown when `/dev/uinput` cannot be opened.
pub const NO_UINPUT: &str = "The trackpad needs /dev/uinput on the computer. On NixOS: hardware.uinput.enable = true, and add yourself to the uinput group.";

/// A device error in words a person can act on.
pub fn why(e: io::Error) -> InputError {
    match e.kind() {
        io::ErrorKind::PermissionDenied | io::ErrorKind::NotFound => {
            InputError::Unavailable(NO_UINPUT.into())
        }
        _ => InputError::Failed(format!("the computer could not drive the trackpad: {e}")),
    }
}

/// `/dev/uinput` opens for writing — the probe for every virtual device. Opening the node makes
/// no device; only `UI_DEV_CREATE` does.
fn uinput() -> Result<(), String> {
    OpenOptions::new()
        .write(true)
        .custom_flags(libc::O_NONBLOCK)
        .open("/dev/uinput")
        .map(drop)
        .map_err(|e| why(e).to_string())
}

/// Offer `imp` when `probe` passed.
fn offer<T: ?Sized>(probe: Result<(), String>, imp: impl FnOnce() -> Arc<T>) -> Offer<T> {
    Some(probe.map(|()| imp()))
}

/// Every Linux provider in one: the host's own tools and devices.
pub struct LinuxProvider;

impl Provider for LinuxProvider {
    fn id(&self) -> ProviderId {
        LINUX
    }

    fn touch(&self) -> Offer<dyn Opener<dyn TouchInjector>> {
        offer(uinput(), || {
            Arc::new(|| Ok(Box::new(touchpad::UinputTouch::default()) as Box<dyn TouchInjector>))
                as Arc<dyn Opener<dyn TouchInjector>>
        })
    }

    fn keys(&self) -> Offer<dyn Opener<dyn KeyInjector>> {
        offer(uinput(), || {
            Arc::new(|| {
                keyboard::Keyboard::new()
                    .map(|k| Box::new(k) as Box<dyn KeyInjector>)
                    .map_err(why)
            }) as Arc<dyn Opener<dyn KeyInjector>>
        })
    }

    fn gamepad(&self) -> Offer<dyn Opener<dyn GamepadInjector>> {
        offer(uinput(), wado_compositor::input::gamepad::opener)
    }

    fn text(&self) -> Offer<dyn TextInjector> {
        let probe = wtype::command().and_then(|_| {
            crate::actions::detect::on_path("wtype")
                .then_some(())
                .ok_or_else(|| wtype::NO_WTYPE.to_string())
        });
        offer(probe, || Arc::new(wtype::Wtype) as Arc<dyn TextInjector>)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The platform contract, on the real kernel devices:
    /// `cargo test -p wado platform::linux -- --ignored` with the sandbox off.
    #[test]
    #[ignore = "needs /dev/uinput"]
    fn the_uinput_devices_pass_the_contract() {
        let p = wado_platform::PlatformBuilder::empty()
            .with(LinuxProvider)
            .probe();
        wado_platform::fake::contract::touch(p.touch().unwrap().as_ref());
        wado_platform::fake::contract::keys(p.keys().unwrap().as_ref());
        wado_platform::fake::contract::platform(&p);
    }
}
