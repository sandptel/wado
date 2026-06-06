//! Remote input synthesis: turn [`wado_protocol::InputEvent`]s (parsed by the server
//! from the WebRTC input data channel) into real Wayland `wl_touch` / `wl_keyboard`
//! events on the seat.
//!
//! **Touch + keyboard only, no cursor.** wado never synthesizes pointer motion and
//! renders no cursor — `wl_touch`/`wl_keyboard` are cursorless, which removes the
//! network-cursor-sync problem entirely. This iteration handles a **single** touch
//! contact (down/motion/up/frame); multi-touch, scrolling, and dragging come later
//! (the wire format already carries a contact `id`). See `INPUT_CHALLENGES.md`.
//!
//! Coordinates arrive normalized 0..1 (the client maps them against the displayed
//! video rect) and are scaled here to the output's logical geometry, so taps land 1:1.
//! Keyboard `code` is a Linux evdev keycode; xkb wants evdev+8 (see `Keycode::new`).

use smithay::{
    backend::input::KeyState,
    input::{
        keyboard::{FilterResult, Keycode},
        touch::{DownEvent, MotionEvent, UpEvent},
    },
    utils::{Logical, Point, SERIAL_COUNTER},
};

use wado_protocol::{InputEvent, TouchPhase};

use crate::Wado;

impl Wado {
    /// Apply one remote input event to the seat. No-op when no session is active (there
    /// is no output/surface to target). Called from the panic-guarded input source in
    /// [`crate::build`].
    pub fn handle_remote_input(&mut self, ev: InputEvent) {
        if !self.session_active {
            tracing::debug!(?ev, "input dropped — no active session");
            return;
        }
        tracing::debug!(?ev, "synthesizing input");
        let serial = SERIAL_COUNTER.next_serial();
        let time = self.start_time.elapsed().as_millis() as u32;

        match ev {
            InputEvent::Key { code, pressed } => {
                let state = if pressed { KeyState::Pressed } else { KeyState::Released };
                let keyboard = self.seat.get_keyboard().unwrap();
                // evdev → xkb keycode is +8 (the broken-X keycode base); see Smithay's
                // libinput backend, which does the same.
                keyboard.input::<(), _>(
                    self,
                    Keycode::new(code + 8),
                    state,
                    serial,
                    time,
                    |_, _, _| FilterResult::Forward,
                );
            }
            InputEvent::Touch { id, phase, x, y } => {
                let Some(loc) = self.touch_location(x, y) else {
                    return;
                };
                let slot = Some(id).into();
                let touch = self.seat.get_touch().unwrap();
                match phase {
                    TouchPhase::Down => {
                        let focus = self.surface_under(loc);
                        // Tap-to-focus: raise the touched window and give it keyboard focus.
                        self.focus_window_at(loc, serial);
                        touch.down(self, focus, &DownEvent { slot, location: loc, serial, time });
                        touch.frame(self);
                    }
                    TouchPhase::Motion => {
                        let focus = self.surface_under(loc);
                        touch.motion(self, focus, &MotionEvent { slot, location: loc, time });
                        touch.frame(self);
                    }
                    TouchPhase::Up => {
                        touch.up(self, &UpEvent { slot, serial, time });
                        touch.frame(self);
                    }
                }
            }
        }
    }

    /// Map normalized 0..1 client coordinates to a point in the output's logical space.
    /// `None` if there is no mapped output yet.
    fn touch_location(&self, x: f64, y: f64) -> Option<Point<f64, Logical>> {
        let output = self.space.outputs().next()?;
        let geo = self.space.output_geometry(output)?;
        let lx = geo.loc.x as f64 + x.clamp(0.0, 1.0) * geo.size.w as f64;
        let ly = geo.loc.y as f64 + y.clamp(0.0, 1.0) * geo.size.h as f64;
        Some((lx, ly).into())
    }

    /// Raise the window under `loc` and set keyboard focus to it (tap-to-focus). Mirrors
    /// the pointer-button focus logic in `input/physical.rs`.
    fn focus_window_at(&mut self, loc: Point<f64, Logical>, serial: smithay::utils::Serial) {
        if let Some((window, _)) = self.space.element_under(loc).map(|(w, l)| (w.clone(), l)) {
            self.space.raise_element(&window, true);
            let keyboard = self.seat.get_keyboard().unwrap();
            keyboard.set_focus(self, Some(window.toplevel().unwrap().wl_surface().clone()), serial);
            self.space.elements().for_each(|w| {
                w.toplevel().unwrap().send_pending_configure();
            });
        }
    }
}
