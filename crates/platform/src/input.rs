//! Input devices — the hot path. Small synchronous traits that never await: a touch frame or a
//! key tap goes straight to the device, from the one input lane that owns it.
//!
//! Devices are made through an [`Opener`], not at probe time: a device is visible to the whole
//! machine (a uinput device on Linux), so it exists only while a phone is driving it. Codes are
//! Linux evdev `KEY_*` / `BTN_*` / `ABS_*` everywhere; another platform maps them to its own.

use wado_protocol::pad::{Contact, Mod};

use crate::{Fut, Res};

/// Why a device call failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputError {
    /// The device cannot be had on this computer at all (no permission, no driver) — retrying
    /// sixty times a second will not change that. The words say how to fix it.
    Unavailable(String),
    /// This call failed; the next may not.
    Failed(String),
}

impl InputError {
    pub fn message(&self) -> &str {
        match self {
            InputError::Unavailable(m) | InputError::Failed(m) => m,
        }
    }
}

impl std::fmt::Display for InputError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.message())
    }
}

/// A device call's outcome.
pub type InputRes = Result<(), InputError>;

/// Makes one device. The registry holds the opener; the input lane holds what it opens.
pub trait Opener<D: ?Sized>: Send + Sync {
    fn open(&self) -> Result<Box<D>, InputError>;
}

impl<D: ?Sized, F> Opener<D> for F
where
    F: Fn() -> Result<Box<D>, InputError> + Send + Sync,
{
    fn open(&self) -> Result<Box<D>, InputError> {
        self()
    }
}

/// A multi-touch touchpad. Positions are `0..=65535` across the pad, whatever its size.
pub trait TouchInjector: Send {
    /// The pad is `w` × `h` CSS px. A size that differs from the current one may remake the
    /// device (axes are fixed at creation on Linux); every finger must already be up.
    fn resize(&mut self, w: u16, h: u16) -> InputRes;
    /// The fingers down now — each `(id, x, y)`. An empty frame lifts every finger.
    fn frame(&mut self, contacts: &[Contact]) -> InputRes;
    /// Lift every finger.
    fn lift_all(&mut self) -> InputRes {
        self.frame(&[])
    }
}

/// A keyboard: named keys and chords. Dropping it releases whatever it holds.
pub trait KeyInjector: Send {
    /// Tap `code` under `mods`: mods down, key down and up, mods up. A mod already held stays.
    fn tap(&mut self, mods: &[Mod], code: u16) -> InputRes;
    /// Hold exactly `mods` down, releasing any held mod not in it.
    fn hold(&mut self, mods: &[Mod]) -> InputRes;
    /// Let go of everything held.
    fn release_all(&mut self) -> InputRes {
        self.hold(&[])
    }
}

/// One gamepad report: a button, or an axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GamepadEvent {
    /// `BTN_*` pressed or released.
    Button { code: u16, pressed: bool },
    /// `ABS_*` moved to `value` (the device clamps it to the axis' range).
    Axis { code: u16, value: i32 },
}

/// A game controller. Failures are the device's to log: there is nothing useful to do about a
/// lost report mid-game.
pub trait GamepadInjector: Send {
    fn report(&mut self, ev: GamepadEvent);
}

/// Typing committed text — arbitrary Unicode, not a per-character key lookup. Not on the hot
/// path: it may take a helper process, so it is async.
pub trait TextInjector: Send + Sync {
    fn type_text(&self, text: String) -> Fut<'_, Res<()>>;
}
