use smithay::{
    desktop::{PopupKind, PopupManager, Space, Window, find_popup_root_surface, get_popup_toplevel_coords},
    input::{
        Seat,
        pointer::{Focus, GrabStartData as PointerGrabStartData},
        touch::GrabStartData as TouchGrabStartData,
    },
    reexports::{
        wayland_protocols::xdg::shell::server::xdg_toplevel,
        wayland_server::{
            Resource,
            protocol::{wl_output, wl_seat, wl_surface::WlSurface},
        },
    },
    utils::{Rectangle, Serial},
    wayland::{
        compositor::with_states,
        shell::xdg::{
            PopupSurface, PositionerState, ToplevelSurface, XdgShellHandler, XdgShellState,
            XdgToplevelSurfaceData,
        },
    },
};

use crate::{
    Wado,
    grabs::{MoveSurfaceGrab, ResizeSurfaceGrab, TouchMoveSurfaceGrab, TouchResizeSurfaceGrab},
};

impl XdgShellHandler for Wado {
    fn xdg_shell_state(&mut self) -> &mut XdgShellState {
        &mut self.xdg_shell_state
    }

    fn new_toplevel(&mut self, surface: ToplevelSurface) {
        let window = Window::new_wayland_window(surface);
        self.place_new_toplevel(window);
    }

    fn new_popup(&mut self, surface: PopupSurface, _positioner: PositionerState) {
        self.unconstrain_popup(&surface);
        let _ = self.popups.track_popup(PopupKind::Xdg(surface));
    }

    fn reposition_request(&mut self, surface: PopupSurface, positioner: PositionerState, token: u32) {
        surface.with_pending_state(|state| {
            let geometry = positioner.get_geometry();
            state.geometry = geometry;
            state.positioner = positioner;
        });
        self.unconstrain_popup(&surface);
        surface.send_repositioned(token);
    }

    fn move_request(&mut self, surface: ToplevelSurface, seat: wl_seat::WlSeat, serial: Serial) {
        let seat = Seat::from_resource(&seat).unwrap();
        let wl_surface = surface.wl_surface();

        let Some(window) = self
            .space
            .elements()
            .find(|w| w.toplevel().unwrap().wl_surface() == wl_surface)
            .cloned()
        else {
            return;
        };
        let initial_window_location = self.space.element_location(&window).unwrap();

        // Honour the move whether the client started it from a pointer (e.g. a desktop
        // mouse) or a touch contact. wado drives touch, so the touch path is the live one.
        if let Some(start_data) = check_grab(&seat, wl_surface, serial) {
            let pointer = seat.get_pointer().unwrap();
            let grab = MoveSurfaceGrab { start_data, window, initial_window_location };
            pointer.set_grab(self, grab, serial, Focus::Clear);
        } else if let Some(start_data) = check_grab_touch(&seat, wl_surface, serial) {
            let touch = seat.get_touch().unwrap();
            let grab = TouchMoveSurfaceGrab { start_data, window, initial_window_location };
            touch.set_grab(self, grab, serial);
        }
    }

    fn resize_request(
        &mut self,
        surface: ToplevelSurface,
        seat: wl_seat::WlSeat,
        serial: Serial,
        edges: xdg_toplevel::ResizeEdge,
    ) {
        let seat = Seat::from_resource(&seat).unwrap();
        let wl_surface = surface.wl_surface();

        let Some(window) = self
            .space
            .elements()
            .find(|w| w.toplevel().unwrap().wl_surface() == wl_surface)
            .cloned()
        else {
            return;
        };
        let initial_window_location = self.space.element_location(&window).unwrap();
        let initial_window_size = window.geometry().size;
        let initial_rect = Rectangle::new(initial_window_location, initial_window_size);

        if let Some(start_data) = check_grab(&seat, wl_surface, serial) {
            let pointer = seat.get_pointer().unwrap();
            surface.with_pending_state(|state| {
                state.states.set(xdg_toplevel::State::Resizing);
            });
            surface.send_pending_configure();
            let grab = ResizeSurfaceGrab::start(start_data, window, edges.into(), initial_rect);
            pointer.set_grab(self, grab, serial, Focus::Clear);
        } else if let Some(start_data) = check_grab_touch(&seat, wl_surface, serial) {
            let touch = seat.get_touch().unwrap();
            surface.with_pending_state(|state| {
                state.states.set(xdg_toplevel::State::Resizing);
            });
            surface.send_pending_configure();
            let grab = TouchResizeSurfaceGrab::start(start_data, window, edges.into(), initial_rect);
            touch.set_grab(self, grab, serial);
        }
    }

    /// The `output` argument is ignored: a wado session has exactly one output by construction
    /// (invariant #8), so there is nothing to choose between. See [`crate::fullscreen`].
    fn fullscreen_request(
        &mut self,
        surface: ToplevelSurface,
        _output: Option<wl_output::WlOutput>,
    ) {
        self.set_fullscreen(&surface, true);
    }

    fn unfullscreen_request(&mut self, surface: ToplevelSurface) {
        self.set_fullscreen(&surface, false);
    }

    fn grab(&mut self, _surface: PopupSurface, _seat: wl_seat::WlSeat, _serial: Serial) {}
}

fn check_grab(
    seat: &Seat<Wado>,
    surface: &WlSurface,
    serial: Serial,
) -> Option<PointerGrabStartData<Wado>> {
    let pointer = seat.get_pointer()?;

    if !pointer.has_grab(serial) {
        return None;
    }

    let start_data = pointer.grab_start_data()?;

    let (focus, _) = start_data.focus.as_ref()?;
    if !focus.id().same_client_as(&surface.id()) {
        return None;
    }

    Some(start_data)
}

/// Touch analogue of [`check_grab`]: validate that the move/resize request comes from the
/// touch contact that currently holds the grab and is focused on this client's surface.
fn check_grab_touch(
    seat: &Seat<Wado>,
    surface: &WlSurface,
    serial: Serial,
) -> Option<TouchGrabStartData<Wado>> {
    let touch = seat.get_touch()?;

    if !touch.has_grab(serial) {
        return None;
    }

    let start_data = touch.grab_start_data()?;

    let (focus, _) = start_data.focus.as_ref()?;
    if !focus.id().same_client_as(&surface.id()) {
        return None;
    }

    Some(start_data)
}

pub fn handle_commit(popups: &mut PopupManager, space: &Space<Window>, surface: &WlSurface) {
    if let Some(window) = space
        .elements()
        .find(|w| w.toplevel().unwrap().wl_surface() == surface)
        .cloned()
    {
        let initial_configure_sent = with_states(surface, |states| {
            states
                .data_map
                .get::<XdgToplevelSurfaceData>()
                .unwrap()
                .lock()
                .unwrap()
                .initial_configure_sent
        });

        if !initial_configure_sent {
            window.toplevel().unwrap().send_configure();
        }
    }

    popups.commit(surface);
    if let Some(popup) = popups.find_popup(surface) {
        match popup {
            PopupKind::Xdg(ref xdg) => {
                if !xdg.is_initial_configure_sent() {
                    xdg.send_configure().expect("initial configure failed");
                }
            }
            PopupKind::InputMethod(ref _input_method) => {}
        }
    }
}

impl Wado {
    fn unconstrain_popup(&self, popup: &PopupSurface) {
        let Ok(root) = find_popup_root_surface(&PopupKind::Xdg(popup.clone())) else {
            return;
        };
        let Some(window) = self
            .space
            .elements()
            .find(|w| w.toplevel().unwrap().wl_surface() == &root)
        else {
            return;
        };

        // All three were `unwrap()`. The first is reachable and fatal: `stop_session` unmaps
        // the output, while mapped windows are never removed — so any application still alive
        // after a session ends (one started from the shell or from `Exec` inherits
        // `WAYLAND_DISPLAY` and is not in `app_processes`, so nothing kills it) panics the
        // compositor on its next menu, tooltip or combo box. The Wayland dispatch source is
        // not `catch_unwind`-guarded, so that unwinds out of `event_loop.run` and takes the
        // whole daemon with it — and because `main` unwinds, `stop_session` never runs and
        // every application the session launched survives the daemon's death.
        //
        // Unconstraining a popup against an output that no longer exists has no meaningful
        // answer, so leaving the client's requested geometry alone is the right no-op. Every
        // other site that reads the output already handles this (`placement.rs`,
        // `input/common.rs`, `handlers/mod.rs`); only this one did not.
        let (Some(output), Some(window_geo)) = (
            self.space.outputs().next(),
            self.space.element_geometry(window),
        ) else {
            return;
        };
        let Some(output_geo) = self.space.output_geometry(output) else {
            return;
        };

        let mut target = output_geo;
        target.loc -= get_popup_toplevel_coords(&PopupKind::Xdg(popup.clone()));
        target.loc -= window_geo.loc;

        popup.with_pending_state(|state| {
            state.geometry = state.positioner.get_unconstrained_geometry(target);
        });
    }
}
