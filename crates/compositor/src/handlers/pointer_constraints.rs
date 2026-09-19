//! `zwp_pointer_constraints_v1`: an application asking to keep the pointer.
//!
//! This is the half of pointer lock that lives in the protocol rather than in the browser. A
//! game does not want pointer *events* at a position — it wants the pointer to stop having one,
//! so that mouse movement becomes camera movement with no edge to run into. SDL, GLFW and every
//! engine built on them look for **both** this global and `zwp_relative_pointer_v1` before they
//! will turn relative mouse mode on; with either missing they fall back to warping the pointer
//! back to the centre, which is the jitter people recognise as "the mouse is fighting me".
//!
//! **Granted on request, always.** A desktop compositor weighs a lock against the risk of a
//! window trapping the pointer where the user cannot escape. That risk does not exist here: the
//! escape hatch is not a compositor policy but the browser's, which releases its own pointer
//! lock on Escape and cannot be talked out of it. See `js/input_lock.js`.
//!
//! ponytail: the cursor position hint is accepted and dropped. It says where to put the cursor
//! when the lock ends, and it exists so the pointer does not teleport when a game returns to its
//! menu — but wado draws no cursor, and the client goes straight back to sending absolute
//! positions the moment the lock ends, so the very next mouse move overwrites whatever this
//! would have set. Honouring it would be a write nothing ever reads.

use smithay::{
    input::pointer::PointerHandle,
    reexports::wayland_server::protocol::wl_surface::WlSurface,
    utils::{Logical, Point},
    wayland::pointer_constraints::{PointerConstraintsHandler, with_pointer_constraint},
};

use crate::Wado;

impl PointerConstraintsHandler for Wado {
    fn new_constraint(&mut self, surface: &WlSurface, pointer: &PointerHandle<Self>) {
        // Only if the pointer is already on that surface. A constraint created for a surface
        // the pointer is elsewhere on stays inactive until the pointer arrives — which is what
        // `pointer_relative` re-checks on every move.
        if pointer.current_focus().as_ref() != Some(surface) {
            return;
        }
        with_pointer_constraint(surface, pointer, |constraint| {
            if let Some(constraint) = constraint {
                constraint.activate();
            }
        });
    }

    fn remove_constraint(&mut self, _surface: &WlSurface, _pointer: &PointerHandle<Self>) {}

    fn cursor_position_hint(
        &mut self,
        _surface: &WlSurface,
        _pointer: &PointerHandle<Self>,
        _location: Point<f64, Logical>,
    ) {
    }
}
