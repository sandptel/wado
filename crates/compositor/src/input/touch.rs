//! Touch input synthesis: [`wado_protocol::InputEvent::Touch`]/`CancelTouch` → `wl_touch`.
//!
//! Coordinates arrive normalized 0..1 and are mapped to the output's logical space, so taps
//! land 1:1. Multiple simultaneous contacts pass through per `id` (multi-touch). Touch is
//! the input model for real touchscreens; a desktop mouse uses the cursorless pointer path
//! (`input/pointer.rs`) instead.

use smithay::input::touch::{DownEvent, MotionEvent, UpEvent};

use wado_protocol::TouchPhase;

use crate::Wado;

impl Wado {
    /// Synthesize one touch contact lifecycle event.
    pub(crate) fn touch(&mut self, id: u32, phase: TouchPhase, x: f64, y: f64) {
        let Some(loc) = self.map_point(x, y) else {
            return;
        };
        let (serial, time) = self.input_clock();
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

    /// Cancel the current touch sequence when a gesture takes over (e.g. a long-press
    /// promotes to a window move/right-click). `wl_touch` cancel is **global** (all live
    /// contacts) — acceptable for our single-primary gesture model.
    pub(crate) fn touch_cancel(&mut self) {
        self.seat.get_touch().unwrap().cancel(self);
    }
}
