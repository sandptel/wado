//! Touch counterpart of [`MoveSurfaceGrab`](super::MoveSurfaceGrab): the interactive
//! window move started when a client asks for one via `xdg_toplevel.move` (CSD titlebar
//! drag) while the gesture is driven by `wl_touch`. Mirrors the pointer grab — `motion`
//! repositions the window, and the grab ends when the initiating contact lifts.

use crate::Wado;
use smithay::{
    desktop::Window,
    input::touch::{
        DownEvent, GrabStartData, MotionEvent, OrientationEvent, ShapeEvent, TouchGrab,
        TouchInnerHandle, UpEvent,
    },
    utils::{Logical, Point, Serial},
};

pub struct TouchMoveSurfaceGrab {
    pub start_data: GrabStartData<Wado>,
    pub window: Window,
    pub initial_window_location: Point<i32, Logical>,
}

impl TouchGrab<Wado> for TouchMoveSurfaceGrab {
    fn down(
        &mut self,
        data: &mut Wado,
        handle: &mut TouchInnerHandle<'_, Wado>,
        _focus: Option<(<Wado as smithay::input::SeatHandler>::TouchFocus, Point<f64, Logical>)>,
        event: &DownEvent,
        seq: Serial,
    ) {
        // Additional contacts during a move are dropped (the window keeps following the
        // initiating contact), matching how the shell grabs behave.
        handle.down(data, None, event, seq);
    }

    fn up(&mut self, data: &mut Wado, handle: &mut TouchInnerHandle<'_, Wado>, event: &UpEvent, seq: Serial) {
        handle.up(data, event, seq);
        // End the move once the contact that started it lifts.
        if event.slot == self.start_data.slot {
            handle.unset_grab(self, data);
        }
    }

    fn motion(
        &mut self,
        data: &mut Wado,
        handle: &mut TouchInnerHandle<'_, Wado>,
        _focus: Option<(<Wado as smithay::input::SeatHandler>::TouchFocus, Point<f64, Logical>)>,
        event: &MotionEvent,
        seq: Serial,
    ) {
        // Only the initiating contact drives the move.
        if event.slot != self.start_data.slot {
            return;
        }
        handle.motion(data, None, event, seq);

        let delta = event.location - self.start_data.location;
        let new_location = self.initial_window_location.to_f64() + delta;
        data.space.map_element(self.window.clone(), new_location.to_i32_round(), true);
    }

    fn frame(&mut self, data: &mut Wado, handle: &mut TouchInnerHandle<'_, Wado>, seq: Serial) {
        handle.frame(data, seq)
    }

    fn cancel(&mut self, data: &mut Wado, handle: &mut TouchInnerHandle<'_, Wado>, seq: Serial) {
        handle.cancel(data, seq);
        handle.unset_grab(self, data);
    }

    fn shape(&mut self, data: &mut Wado, handle: &mut TouchInnerHandle<'_, Wado>, event: &ShapeEvent, seq: Serial) {
        handle.shape(data, event, seq)
    }

    fn orientation(
        &mut self,
        data: &mut Wado,
        handle: &mut TouchInnerHandle<'_, Wado>,
        event: &OrientationEvent,
        seq: Serial,
    ) {
        handle.orientation(data, event, seq)
    }

    fn start_data(&self) -> &GrabStartData<Wado> {
        &self.start_data
    }

    fn unset(&mut self, _data: &mut Wado) {}
}
