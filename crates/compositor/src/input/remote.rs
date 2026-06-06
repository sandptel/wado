//! Remote input synthesis: turn [`wado_protocol::InputEvent`]s (parsed by the server
//! from the WebRTC input data channel) into real Wayland `wl_touch` / `wl_keyboard`
//! events on the seat.
//!
//! **No on-screen cursor.** wado never renders a cursor. `Touch`/`Key` map to
//! `wl_touch`/`wl_keyboard`. Scrolling and the long-press right-click go through
//! `wl_pointer` (the only Wayland path for axis/secondary-click) — we focus the surface
//! under the point and emit the event, but still draw no cursor. `WindowDrag` is a
//! compositor-managed interactive window move (long-press-drag or "move mode") and never
//! reaches the app. Multi-touch contacts pass through per-`id`. See `INPUT_CHALLENGES.md`.
//!
//! Coordinates arrive normalized 0..1 (the client maps them against the displayed
//! video rect) and are scaled here to the output's logical geometry, so taps land 1:1.
//! Keyboard `code` is a Linux evdev keycode; xkb wants evdev+8 (see `Keycode::new`).

use smithay::{
    backend::input::{Axis, AxisSource, ButtonState, KeyState},
    input::{
        keyboard::{FilterResult, Keycode},
        pointer::{AxisFrame, ButtonEvent, MotionEvent as PointerMotionEvent},
        touch::{DownEvent, MotionEvent, UpEvent},
    },
    utils::{Logical, Point, SERIAL_COUNTER},
};

use wado_protocol::{InputEvent, PointerButton, TouchPhase};

use crate::{Wado, state::WindowMove};

/// `BTN_RIGHT` from Linux `input-event-codes.h`; the secondary mouse button apps expect
/// for context menus. The long-press gesture emulates it (see [`Wado::handle_remote_input`]).
const BTN_RIGHT: u32 = 0x111;

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
            InputEvent::CancelTouch { .. } => {
                // wl_touch cancel is global (all live contacts) — fine for our single-primary
                // gesture takeover; documented as a multi-touch caveat.
                self.seat.get_touch().unwrap().cancel(self);
            }
            InputEvent::Scroll { x, y, dx, dy } => {
                let Some(loc) = self.touch_location(x, y) else {
                    return;
                };
                let pointer = self.seat.get_pointer().unwrap();
                // Focus the surface under the wheel position (no cursor is rendered) so the
                // axis frame is delivered to the right client.
                let under = self.surface_under(loc);
                pointer.motion(self, under, &PointerMotionEvent { location: loc, serial, time });

                // Browser wheel deltas are pixels; forward as continuous axis values plus a
                // v120 discrete step (one notch = 120) so both smooth and stepped scrollers work.
                let mut frame = AxisFrame::new(time).source(AxisSource::Wheel);
                if dx != 0.0 {
                    frame = frame.value(Axis::Horizontal, dx);
                    frame = frame.v120(Axis::Horizontal, (dx.signum() * 120.0) as i32);
                }
                if dy != 0.0 {
                    frame = frame.value(Axis::Vertical, dy);
                    frame = frame.v120(Axis::Vertical, (dy.signum() * 120.0) as i32);
                }
                pointer.axis(self, frame);
                pointer.frame(self);
            }
            InputEvent::Button { x, y, button, pressed } => {
                let Some(loc) = self.touch_location(x, y) else {
                    return;
                };
                let code = match button {
                    PointerButton::Right => BTN_RIGHT,
                };
                let state = if pressed { ButtonState::Pressed } else { ButtonState::Released };
                // Raise + focus the target on press, then deliver the button via the pointer
                // focused at `loc` (still no rendered cursor).
                if pressed {
                    self.focus_window_at(loc, serial);
                }
                let pointer = self.seat.get_pointer().unwrap();
                let under = self.surface_under(loc);
                pointer.motion(self, under, &PointerMotionEvent { location: loc, serial, time });
                pointer.button(self, &ButtonEvent { button: code, state, serial, time });
                pointer.frame(self);
            }
            InputEvent::WindowDrag { phase, x, y } => {
                let Some(loc) = self.touch_location(x, y) else {
                    return;
                };
                match phase {
                    TouchPhase::Down => {
                        if let Some((window, _)) =
                            self.space.element_under(loc).map(|(w, l)| (w.clone(), l))
                        {
                            self.space.raise_element(&window, true);
                            self.focus_window_at(loc, serial);
                            let start_win = self
                                .space
                                .element_location(&window)
                                .unwrap_or_else(|| (0, 0).into());
                            self.window_move = Some(WindowMove { window, start_ptr: loc, start_win });
                        }
                    }
                    TouchPhase::Motion => {
                        if let Some(mv) = &self.window_move {
                            let delta = loc - mv.start_ptr;
                            let new_loc = (mv.start_win.to_f64() + delta).to_i32_round();
                            let window = mv.window.clone();
                            self.space.map_element(window, new_loc, true);
                        }
                    }
                    TouchPhase::Up => {
                        self.window_move = None;
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
