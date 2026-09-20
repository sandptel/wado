//! Remote input **dispatcher**. `wado_protocol::InputEvent`s (parsed by the server from the
//! WebRTC input data channel) arrive here and are routed to the per-feature synthesizers —
//! each event family's Wayland synthesis lives in its own single-job module:
//!
//! - [`keyboard`](super::keyboard) — `Key` → `wl_keyboard`
//! - [`touch`](super::touch) — `Touch` / `CancelTouch` → `wl_touch` (touchscreens)
//! - [`pointer`](super::pointer) — `PointerMotion` / `Button` / `Scroll` → `wl_pointer` (mouse,
//!   cursorless)
//! - [`relative`](super::relative) — `PointerRelative` → `zwp_relative_pointer_v1` (pointer lock)
//! - [`pinch`](super::pinch) — `Pinch` → `zwp_pointer_gestures_v1`
//! - [`window_drag`](super::window_drag) — `WindowDrag` → compositor window move
//!
//! This file only dispatches; it holds no synthesis logic of its own.

use wado_protocol::InputEvent;

use crate::Wado;

impl Wado {
    /// Route one remote input event to its synthesizer. No-op when no session is active
    /// (there is no output/surface to target). Called from the panic-guarded input source
    /// in [`crate::build`].
    pub fn handle_remote_input(&mut self, ev: InputEvent) {
        if !self.session_active {
            tracing::debug!(?ev, "input dropped — no active session");
            return;
        }
        tracing::trace!(?ev, "synthesizing input");
        match ev {
            InputEvent::Key { code, pressed } => self.key(code, pressed),
            InputEvent::Touch { id, phase, x, y } => self.touch(id, phase, x, y),
            InputEvent::CancelTouch { .. } => self.touch_cancel(),
            InputEvent::PointerMotion { x, y } => self.pointer_motion(x, y),
            InputEvent::PointerRelative { dx, dy } => self.pointer_relative(dx, dy),
            InputEvent::Button { x, y, button, pressed } => {
                self.pointer_button(x, y, button, pressed)
            }
            InputEvent::Scroll { x, y, dx, dy, source, stop } => {
                self.pointer_scroll(x, y, dx, dy, source, stop)
            }
            InputEvent::Pinch { phase, x, y, scale, rotation } => {
                self.pinch(phase, x, y, scale, rotation)
            }
            InputEvent::WindowDrag { phase, x, y } => self.window_drag(phase, x, y),
            InputEvent::GamepadButton { code, pressed } => {
                // Discrete and human-paced, so it can be logged at info: this is the one
                // trace that says a stroke crossed the wire and reached the host device.
                tracing::info!(target: "wado::gamepad", "gamepad button {code:#x} {}",
                    if pressed { "down" } else { "up" });
                if let Some(pad) = self.virtual_gamepad() {
                    pad.button(code, pressed);
                }
            }
            InputEvent::GamepadAxis { code, value } => {
                // Debug, not info: a stick at rest still sends sixty of these a second, and
                // a log that scrolls is a log nobody reads the button presses out of.
                tracing::debug!(target: "wado::gamepad", "gamepad axis {code:#x} = {value}");
                if let Some(pad) = self.virtual_gamepad() {
                    pad.axis(code, value);
                }
            }
            // Never reaches here in practice: the server answers Ping itself and does not
            // forward it, precisely so the probe measures the input path without the
            // compositor's render loop in the way. Ignored rather than warned so a stray
            // one can't spam the log.
            InputEvent::Ping { .. } => {}
        }
    }

    /// The session's virtual gamepad, opening it on first use.
    ///
    /// Lazy because the device is visible to the whole machine: a session nobody opened the
    /// on-screen pad in must not leave a phantom controller behind for every other application
    /// on the host to enumerate. The failure is latched so a held stick logs the permission
    /// problem once rather than sixty times a second.
    fn virtual_gamepad(&mut self) -> Option<&mut crate::input::gamepad::Gamepad> {
        if self.gamepad.is_none() && !self.gamepad_failed {
            match crate::input::gamepad::Gamepad::open() {
                Ok(pad) => self.gamepad = Some(pad),
                Err(e) => {
                    self.gamepad_failed = true;
                    tracing::error!("virtual gamepad unavailable: {e}");
                }
            }
        }
        self.gamepad.as_mut()
    }
}
