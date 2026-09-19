//! What keyboard focus can point at.
//!
//! One job: the keyboard focus target type and its conversions. Nothing else belongs here.
//!
//! **Why this exists at all.** `PopupManager::grab_popup` — the only way to implement
//! `XdgShellHandler::grab`, and therefore the only way a menu dismisses when you tap outside it
//! — is bounded on `SeatHandler::KeyboardFocus: From<PopupKind>`. wado used `WlSurface`, and
//! `impl From<PopupKind> for WlSurface` is forbidden by the orphan rule: both types are foreign.
//! A local enum breaks that deadlock, and it is the *only* reason to introduce one.
//!
//! **Deliberately only the keyboard.** `grab_popup` also wants
//! `PointerFocus: From<KeyboardFocus>`, and that direction is legal with `WlSurface` as the
//! pointer focus because the local type sits in the parameter position — so
//! `PointerFocus`/`TouchFocus` stay `WlSurface` and the pointer, touch and grab code is
//! untouched. The full anvil-style refactor would have been 63 call sites across 14 files; this
//! is 8. ponytail: the upgrade path is a matching `PointerFocusTarget`, needed only if wado ever
//! wants layer-shell or a real cursor to take pointer focus distinctly from a surface.

use smithay::{
    desktop::PopupKind,
    input::{
        Seat, SeatHandler,
        keyboard::{KeyboardTarget, KeysymHandle, ModifiersState},
    },
    reexports::wayland_server::protocol::wl_surface::WlSurface,
    utils::{IsAlive, Serial},
    wayland::seat::WaylandFocus,
};
use smithay::backend::input::KeyState;

use crate::Wado;

/// The thing holding keyboard focus: an ordinary window's surface, or a popup.
///
/// The distinction is what a popup grab needs — during a grab, focus belongs to the popup and
/// must be given back when it ends — and it is carried no further than that. Everything that
/// only wants "which surface" calls [`WaylandFocus::wl_surface`] or converts.
#[derive(Debug, Clone, PartialEq)]
pub enum KeyboardFocusTarget {
    /// A toplevel's surface, which is what focus meant before popups could hold it.
    Surface(WlSurface),
    /// A popup holding focus for the duration of its grab.
    Popup(PopupKind),
}

impl From<PopupKind> for KeyboardFocusTarget {
    fn from(p: PopupKind) -> Self {
        KeyboardFocusTarget::Popup(p)
    }
}

impl From<WlSurface> for KeyboardFocusTarget {
    fn from(s: WlSurface) -> Self {
        KeyboardFocusTarget::Surface(s)
    }
}

/// The conversion `grab_popup` needs from keyboard focus to pointer focus. Legal where
/// `From<PopupKind> for WlSurface` is not: the local type is in the parameter position.
impl From<KeyboardFocusTarget> for WlSurface {
    fn from(t: KeyboardFocusTarget) -> Self {
        match t {
            KeyboardFocusTarget::Surface(s) => s,
            KeyboardFocusTarget::Popup(p) => p.wl_surface().clone(),
        }
    }
}

impl IsAlive for KeyboardFocusTarget {
    fn alive(&self) -> bool {
        match self {
            KeyboardFocusTarget::Surface(s) => s.alive(),
            KeyboardFocusTarget::Popup(p) => p.alive(),
        }
    }
}

impl WaylandFocus for KeyboardFocusTarget {
    fn wl_surface(&self) -> Option<std::borrow::Cow<'_, WlSurface>> {
        match self {
            KeyboardFocusTarget::Surface(s) => Some(std::borrow::Cow::Borrowed(s)),
            KeyboardFocusTarget::Popup(p) => Some(std::borrow::Cow::Borrowed(p.wl_surface())),
        }
    }
}

/// Delegated wholesale to the underlying surface: both variants are a `wl_surface` as far as the
/// keyboard protocol is concerned, and a popup that has focus receives key events exactly as a
/// toplevel does. The enum exists for the *grab*, not to change what a key press means.
impl KeyboardTarget<Wado> for KeyboardFocusTarget {
    fn enter(&self, seat: &Seat<Wado>, data: &mut Wado, keys: Vec<KeysymHandle<'_>>, serial: Serial) {
        KeyboardTarget::enter(&surface_of(self), seat, data, keys, serial)
    }
    fn leave(&self, seat: &Seat<Wado>, data: &mut Wado, serial: Serial) {
        KeyboardTarget::leave(&surface_of(self), seat, data, serial)
    }
    fn key(
        &self,
        seat: &Seat<Wado>,
        data: &mut Wado,
        key: KeysymHandle<'_>,
        state: KeyState,
        serial: Serial,
        time: u32,
    ) {
        KeyboardTarget::key(&surface_of(self), seat, data, key, state, serial, time)
    }
    fn modifiers(&self, seat: &Seat<Wado>, data: &mut Wado, mods: ModifiersState, serial: Serial) {
        KeyboardTarget::modifiers(&surface_of(self), seat, data, mods, serial)
    }
}

fn surface_of(t: &KeyboardFocusTarget) -> WlSurface {
    match t {
        KeyboardFocusTarget::Surface(s) => s.clone(),
        KeyboardFocusTarget::Popup(p) => p.wl_surface().clone(),
    }
}

/// `<Wado as SeatHandler>::KeyboardFocus`, named once so the swap in `handlers/mod.rs` reads as
/// a type alias rather than a scattered concrete type.
pub type KeyboardFocus = <Wado as SeatHandler>::KeyboardFocus;
