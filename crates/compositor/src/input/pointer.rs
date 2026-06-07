//! Cursorless pointer synthesis: [`wado_protocol::InputEvent::PointerMotion`]/`Button`/
//! `Scroll` → `wl_pointer` motion / button / axis.
//!
//! wado renders **no cursor**, but a desktop mouse drives the real `wl_pointer` so apps get
//! the full pointer feature set: hover (menus, tooltips), left/middle/right buttons,
//! click-drag and selections, scroll, and — because button-press sets focus — app-initiated
//! CSD titlebar move / edge resize (which start the pointer grabs in `grabs/`).

use smithay::{
    backend::input::{Axis, AxisSource, ButtonState},
    input::pointer::{AxisFrame, ButtonEvent, MotionEvent},
    reexports::wayland_server::protocol::wl_surface::WlSurface,
};

use wado_protocol::PointerButton;

use crate::Wado;

// Linux `input-event-codes.h` button codes.
const BTN_LEFT: u32 = 0x110;
const BTN_RIGHT: u32 = 0x111;
const BTN_MIDDLE: u32 = 0x112;

impl Wado {
    /// Absolute pointer motion / hover. Optionally moves keyboard focus with the pointer
    /// (`focus_follows_pointer`). No cursor is drawn.
    pub(crate) fn pointer_motion(&mut self, x: f64, y: f64) {
        let Some(loc) = self.map_point(x, y) else {
            return;
        };
        let (serial, time) = self.input_clock();
        let under = self.surface_under(loc);
        let pointer = self.seat.get_pointer().unwrap();
        pointer.motion(self, under.clone(), &MotionEvent { location: loc, serial, time });
        pointer.frame(self);

        if self.focus_follows_pointer {
            if let Some((surface, _)) = under {
                self.seat.get_keyboard().unwrap().set_focus(self, Some(surface), serial);
            }
        }
    }

    /// A pointer button press/release. Mirrors the canonical click-to-focus behaviour:
    /// pressing raises + focuses the window under the pointer (or clears focus on empty
    /// space), which is also what makes apps issue `move_request`/`resize_request`.
    pub(crate) fn pointer_button(&mut self, x: f64, y: f64, button: PointerButton, pressed: bool) {
        let Some(loc) = self.map_point(x, y) else {
            return;
        };
        let (serial, time) = self.input_clock();
        let code = match button {
            PointerButton::Left => BTN_LEFT,
            PointerButton::Middle => BTN_MIDDLE,
            PointerButton::Right => BTN_RIGHT,
        };
        let state = if pressed { ButtonState::Pressed } else { ButtonState::Released };

        let under = self.surface_under(loc);
        let pointer = self.seat.get_pointer().unwrap();
        pointer.motion(self, under, &MotionEvent { location: loc, serial, time });

        if state == ButtonState::Pressed && !pointer.is_grabbed() {
            let keyboard = self.seat.get_keyboard().unwrap();
            if let Some((window, _)) = self.space.element_under(loc).map(|(w, l)| (w.clone(), l)) {
                self.space.raise_element(&window, true);
                keyboard.set_focus(
                    self,
                    Some(window.toplevel().unwrap().wl_surface().clone()),
                    serial,
                );
                self.space.elements().for_each(|w| {
                    w.toplevel().unwrap().send_pending_configure();
                });
            } else {
                self.space.elements().for_each(|w| {
                    w.set_activated(false);
                    w.toplevel().unwrap().send_pending_configure();
                });
                keyboard.set_focus(self, Option::<WlSurface>::None, serial);
            }
        }

        pointer.button(self, &ButtonEvent { button: code, state, serial, time });
        pointer.frame(self);
    }

    /// Scroll at a point. `dx`/`dy` are already-normalized pixel deltas; we emit a
    /// **value-only** wheel axis frame (no v120), which GTK/Qt/web/terminals honour as
    /// smooth scroll. Discrete-step emulation is intentionally left out (see CHALLENGES.md).
    pub(crate) fn pointer_scroll(&mut self, x: f64, y: f64, dx: f64, dy: f64) {
        let Some(loc) = self.map_point(x, y) else {
            return;
        };
        let (serial, time) = self.input_clock();
        let under = self.surface_under(loc);
        let pointer = self.seat.get_pointer().unwrap();
        // Focus the surface under the wheel point (no cursor) so the axis lands on it.
        pointer.motion(self, under, &MotionEvent { location: loc, serial, time });
        pointer.frame(self);

        let mut frame = AxisFrame::new(time).source(AxisSource::Wheel);
        let mut any = false;
        if dx != 0.0 {
            frame = frame.value(Axis::Horizontal, dx);
            any = true;
        }
        if dy != 0.0 {
            frame = frame.value(Axis::Vertical, dy);
            any = true;
        }
        if any {
            pointer.axis(self, frame);
            pointer.frame(self);
        }
    }
}
