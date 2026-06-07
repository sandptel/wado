//! Remote input **dispatcher**. `wado_protocol::InputEvent`s (parsed by the server from the
//! WebRTC input data channel) arrive here and are routed to the per-feature synthesizers —
//! each event family's Wayland synthesis lives in its own single-job module:
//!
//! - [`keyboard`](super::keyboard) — `Key` → `wl_keyboard`
//! - [`touch`](super::touch) — `Touch` / `CancelTouch` → `wl_touch` (touchscreens)
//! - [`pointer`](super::pointer) — `PointerMotion` / `Button` / `Scroll` → `wl_pointer` (mouse,
//!   cursorless)
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
            InputEvent::Button { x, y, button, pressed } => {
                self.pointer_button(x, y, button, pressed)
            }
            InputEvent::Scroll { x, y, dx, dy } => self.pointer_scroll(x, y, dx, dy),
            InputEvent::WindowDrag { phase, x, y } => self.window_drag(phase, x, y),
        }
    }
}
