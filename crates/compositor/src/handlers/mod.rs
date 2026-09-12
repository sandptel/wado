mod activation;
mod compositor;
pub(crate) mod content_type;
mod decoration;
mod dmabuf;
mod xdg_shell;

use crate::Wado;

use smithay::input::dnd::{DnDGrab, DndGrabHandler, GrabType, Source};
use smithay::input::pointer::Focus;
use smithay::input::{Seat, SeatHandler, SeatState};
use smithay::reexports::wayland_server::Resource;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::Serial;
use smithay::wayland::compositor::with_states;
use smithay::wayland::fractional_scale::{
    FractionalScaleHandler, with_fractional_scale,
};
use smithay::wayland::output::OutputHandler;
use smithay::wayland::selection::SelectionHandler;
use smithay::wayland::selection::data_device::{
    DataDeviceHandler, DataDeviceState, WaylandDndGrabHandler, set_data_device_focus,
};

impl SeatHandler for Wado {
    type KeyboardFocus = WlSurface;
    type PointerFocus = WlSurface;
    type TouchFocus = WlSurface;

    fn seat_state(&mut self) -> &mut SeatState<Wado> {
        &mut self.seat_state
    }

    fn cursor_image(&mut self, _seat: &Seat<Self>, _image: smithay::input::pointer::CursorImageStatus) {}

    fn focus_changed(&mut self, seat: &Seat<Self>, focused: Option<&WlSurface>) {
        let dh = &self.display_handle;
        let client = focused.and_then(|s| dh.get_client(s.id()).ok());
        set_data_device_focus(dh, seat, client);
    }
}

impl SelectionHandler for Wado {
    type SelectionUserData = ();
}

impl DataDeviceHandler for Wado {
    fn data_device_state(&mut self) -> &mut DataDeviceState {
        &mut self.data_device_state
    }
}

impl DndGrabHandler for Wado {}
impl WaylandDndGrabHandler for Wado {
    fn dnd_requested<S: Source>(
        &mut self,
        source: S,
        _icon: Option<WlSurface>,
        seat: Seat<Self>,
        serial: Serial,
        type_: GrabType,
    ) {
        match type_ {
            GrabType::Pointer => {
                let ptr = seat.get_pointer().unwrap();
                let start_data = ptr.grab_start_data().unwrap();
                let grab = DnDGrab::new_pointer(&self.display_handle, start_data, source, seat);
                ptr.set_grab(self, grab, serial, Focus::Keep);
            }
            GrabType::Touch => {
                source.cancel();
            }
        }
    }
}

impl OutputHandler for Wado {}

impl FractionalScaleHandler for Wado {
    /// Tell a surface the scale it should draw for, the moment it asks.
    ///
    /// Anvil walks subsurface parents and per-window output lists to answer this. wado has
    /// exactly one output for the whole session (invariant #8 — a client's resolution comes
    /// from a fresh `Output`, and there is never a second one), so the answer is always that
    /// output's scale and the walk would be ceremony.
    ///
    /// Answering at all is the point: with no reply a client falls back to `wl_output.scale`,
    /// which is an integer, and a fractional session is right back to the clipped buffers
    /// this protocol exists to prevent.
    fn new_fractional_scale(&mut self, surface: WlSurface) {
        let Some(scale) = self.space.outputs().next().map(|o| o.current_scale().fractional_scale())
        else {
            // No output yet: the session has not started. The surface will be told when one
            // appears — see `headless::start_session`, which pushes to every known surface.
            return;
        };
        // Logged because the *other* branch — the integer `wl_output.scale` a client gets
        // when it cannot speak this protocol — is otherwise unmeasurable: a clipped or soft
        // legacy client looks identical to a correct one until someone squints at it. Every
        // surface that appears here took the fractional path; anything mapped that never
        // does is the population `headless.rs`'s `advertised_integer` actually serves.
        tracing::info!(%scale, "fractional scale requested by a surface");
        with_states(&surface, |states| {
            with_fractional_scale(states, |fs| fs.set_preferred_scale(scale));
        });
    }
}

smithay::delegate_dispatch2!(Wado);
