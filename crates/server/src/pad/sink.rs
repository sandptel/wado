//! The real [`Sink`]: the kernel devices a pad drives, made on first use.
//!
//! The touchpad's axes are fixed when it is created, so a new pad size means a new device (the
//! pad has already lifted every finger by then — `state.rs`). Both devices go when this does.

use std::io;

use wado_protocol::pad::{Contact, Mod};

use super::keys::Keyboard;
use super::state::Sink;
use super::touchpad::Touchpad;

/// Shown when `/dev/uinput` cannot be opened.
pub const NO_UINPUT: &str = "The trackpad needs /dev/uinput on the computer. On NixOS: hardware.uinput.enable = true, and add yourself to the uinput group.";

#[derive(Default)]
pub struct Devices {
    touchpad: Option<Touchpad>,
    keyboard: Option<Keyboard>,
}

impl Devices {
    fn keyboard(&mut self) -> Result<&mut Keyboard, String> {
        if self.keyboard.is_none() {
            self.keyboard = Some(Keyboard::new().map_err(why)?);
        }
        Ok(self.keyboard.as_mut().unwrap())
    }
}

impl Sink for Devices {
    fn touch(&mut self, size: (u16, u16), c: &[Contact]) -> Result<(), String> {
        if self.touchpad.as_ref().is_none_or(|t| t.size() != size) {
            self.touchpad = None; // the old device goes before the new one appears
            self.touchpad = Some(Touchpad::new(size.0, size.1).map_err(why)?);
        }
        self.touchpad.as_mut().unwrap().frame(c).map_err(why)
    }

    fn tap(&mut self, mods: &[Mod], code: u16) -> Result<(), String> {
        self.keyboard()?.tap(mods, code).map_err(why)
    }

    fn hold(&mut self, mods: &[Mod]) -> Result<(), String> {
        self.keyboard()?.hold(mods).map_err(why)
    }

    fn release_all(&mut self) {
        if let Some(k) = self.keyboard.as_mut() {
            let _ = k.release_all();
        }
    }
}

/// A device error in words a person can act on.
fn why(e: io::Error) -> String {
    match e.kind() {
        io::ErrorKind::PermissionDenied | io::ErrorKind::NotFound => NO_UINPUT.into(),
        _ => format!("the computer could not drive the trackpad: {e}"),
    }
}
