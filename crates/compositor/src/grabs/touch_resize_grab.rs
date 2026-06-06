//! Touch counterpart of [`ResizeSurfaceGrab`](super::ResizeSurfaceGrab): the interactive
//! resize started via `xdg_toplevel.resize` (CSD edge drag) while driven by `wl_touch`.
//! It shares the per-surface [`ResizeSurfaceState`] commit machinery with the pointer
//! grab — only the grab-trait plumbing and the end condition (the initiating contact
//! lifting, rather than a button release) differ.

use crate::{Wado, grabs::resize_grab::{ResizeEdge, ResizeSurfaceState}};
use smithay::{
    desktop::Window,
    input::touch::{
        DownEvent, GrabStartData, MotionEvent, OrientationEvent, ShapeEvent, TouchGrab,
        TouchInnerHandle, UpEvent,
    },
    reexports::wayland_protocols::xdg::shell::server::xdg_toplevel,
    utils::{Logical, Point, Rectangle, Serial, Size},
    wayland::{compositor, shell::xdg::SurfaceCachedState},
};

pub struct TouchResizeSurfaceGrab {
    start_data: GrabStartData<Wado>,
    window: Window,
    edges: ResizeEdge,
    initial_rect: Rectangle<i32, Logical>,
    last_window_size: Size<i32, Logical>,
}

impl TouchResizeSurfaceGrab {
    pub fn start(
        start_data: GrabStartData<Wado>,
        window: Window,
        edges: ResizeEdge,
        initial_window_rect: Rectangle<i32, Logical>,
    ) -> Self {
        ResizeSurfaceState::with(window.toplevel().unwrap().wl_surface(), |state| {
            *state = ResizeSurfaceState::Resizing { edges, initial_rect: initial_window_rect };
        });

        Self {
            start_data,
            window,
            edges,
            initial_rect: initial_window_rect,
            last_window_size: initial_window_rect.size,
        }
    }

    /// Recompute and request the new size from the current contact location. Mirrors the
    /// pointer resize grab's `motion` math.
    fn resize_to(&mut self, location: Point<f64, Logical>) {
        let mut delta = location - self.start_data.location;

        let mut new_window_width = self.initial_rect.size.w;
        let mut new_window_height = self.initial_rect.size.h;

        if self.edges.intersects(ResizeEdge::LEFT | ResizeEdge::RIGHT) {
            if self.edges.intersects(ResizeEdge::LEFT) {
                delta.x = -delta.x;
            }
            new_window_width = (self.initial_rect.size.w as f64 + delta.x) as i32;
        }

        if self.edges.intersects(ResizeEdge::TOP | ResizeEdge::BOTTOM) {
            if self.edges.intersects(ResizeEdge::TOP) {
                delta.y = -delta.y;
            }
            new_window_height = (self.initial_rect.size.h as f64 + delta.y) as i32;
        }

        let (min_size, max_size) =
            compositor::with_states(self.window.toplevel().unwrap().wl_surface(), |states| {
                let mut guard = states.cached_state.get::<SurfaceCachedState>();
                let data = guard.current();
                (data.min_size, data.max_size)
            });

        let min_width = min_size.w.max(1);
        let min_height = min_size.h.max(1);
        let max_width = if max_size.w == 0 { i32::MAX } else { max_size.w };
        let max_height = if max_size.h == 0 { i32::MAX } else { max_size.h };

        self.last_window_size = Size::from((
            new_window_width.max(min_width).min(max_width),
            new_window_height.max(min_height).min(max_height),
        ));

        let xdg = self.window.toplevel().unwrap();
        xdg.with_pending_state(|state| {
            state.states.set(xdg_toplevel::State::Resizing);
            state.size = Some(self.last_window_size);
        });
        xdg.send_pending_configure();
    }

    /// Commit the final size and hand the surface over to `resize_grab::handle_commit`.
    fn finish(&self) {
        let xdg = self.window.toplevel().unwrap();
        xdg.with_pending_state(|state| {
            state.states.unset(xdg_toplevel::State::Resizing);
            state.size = Some(self.last_window_size);
        });
        xdg.send_pending_configure();

        ResizeSurfaceState::with(xdg.wl_surface(), |state| {
            *state = ResizeSurfaceState::WaitingForLastCommit {
                edges: self.edges,
                initial_rect: self.initial_rect,
            };
        });
    }
}

impl TouchGrab<Wado> for TouchResizeSurfaceGrab {
    fn down(
        &mut self,
        data: &mut Wado,
        handle: &mut TouchInnerHandle<'_, Wado>,
        _focus: Option<(<Wado as smithay::input::SeatHandler>::TouchFocus, Point<f64, Logical>)>,
        event: &DownEvent,
        seq: Serial,
    ) {
        handle.down(data, None, event, seq);
    }

    fn up(&mut self, data: &mut Wado, handle: &mut TouchInnerHandle<'_, Wado>, event: &UpEvent, seq: Serial) {
        handle.up(data, event, seq);
        if event.slot == self.start_data.slot {
            self.finish();
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
        if event.slot != self.start_data.slot {
            return;
        }
        handle.motion(data, None, event, seq);
        self.resize_to(event.location);
    }

    fn frame(&mut self, data: &mut Wado, handle: &mut TouchInnerHandle<'_, Wado>, seq: Serial) {
        handle.frame(data, seq)
    }

    fn cancel(&mut self, data: &mut Wado, handle: &mut TouchInnerHandle<'_, Wado>, seq: Serial) {
        handle.cancel(data, seq);
        self.finish();
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
