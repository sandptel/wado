//! Two-finger pinch synthesis: [`wado_protocol::InputEvent::Pinch`] →
//! `zwp_pointer_gestures_v1` pinch begin/update/end.
//!
//! Its own file for the same reason `pointer.rs` and `touch.rs` are separate: this owns one
//! job, the magnify/rotate half of a two-contact gesture. The translation half is already
//! `pointer_scroll`, and both fire for the same gesture — that is not a bug, it is what
//! libinput reports for a touchpad pinch and what toolkits are written against.

use smithay::input::pointer::{
    GesturePinchBeginEvent, GesturePinchEndEvent, GesturePinchUpdateEvent, MotionEvent,
};

use wado_protocol::TouchPhase;

use crate::Wado;

impl Wado {
    /// One phase of a pinch. `scale` is absolute against the begin distance; `rotation` is
    /// the delta in degrees since the previous update — see the protocol type, the two are
    /// not measured the same way.
    pub(crate) fn pinch(&mut self, phase: TouchPhase, x: f64, y: f64, scale: f64, rotation: f64) {
        let Some(loc) = self.map_point(x, y) else {
            return;
        };
        let (serial, time) = self.input_clock();
        let pointer = self.seat.get_pointer().unwrap();

        match phase {
            TouchPhase::Down => {
                // A gesture is delivered to whatever the pointer is over, and wado draws no
                // cursor, so the pointer has to be put there first or the begin goes
                // nowhere — silently, with no error anywhere. Same reason `pointer_scroll`
                // opens with a motion.
                let under = self.surface_under(loc);
                pointer.motion(
                    self,
                    under,
                    &MotionEvent {
                        location: loc,
                        serial,
                        time,
                    },
                );
                pointer.frame(self);
                pointer.gesture_pinch_begin(
                    self,
                    &GesturePinchBeginEvent {
                        serial,
                        time,
                        fingers: 2,
                    },
                );
            }
            TouchPhase::Motion => {
                pointer.gesture_pinch_update(
                    self,
                    &GesturePinchUpdateEvent {
                        time,
                        // Deliberately zero. `delta` is how far the gesture's centre moved,
                        // which is the translation — and the translation is already being
                        // sent as a scroll axis by the same client gesture. Reporting it
                        // here too would have a toolkit pan twice for one drag.
                        delta: (0.0, 0.0).into(),
                        scale,
                        rotation,
                    },
                );
            }
            TouchPhase::Up => {
                pointer.gesture_pinch_end(
                    self,
                    &GesturePinchEndEvent {
                        serial,
                        time,
                        cancelled: false,
                    },
                );
            }
        }
    }
}
