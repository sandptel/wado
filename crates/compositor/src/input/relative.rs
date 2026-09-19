//! [`wado_protocol::InputEvent::PointerRelative`] → `zwp_relative_pointer_v1`, plus the
//! absolute pointer that has to keep moving alongside it.
//!
//! **Why a second kind of motion exists at all.** Every other pointer event here carries a
//! position, which is right for a phone and for a desktop mouse driving a cursor. It is wrong
//! for a game: a first-person camera turns by the *distance* the mouse moved, and a position
//! normalised against the video rect cannot express a movement that runs past the edge of it.
//! That is the whole of the "the pointer escapes the window and the camera stops turning"
//! problem — the client's pointer hits the side of the browser window and the positions stop
//! changing, so the camera stops, while the real mouse keeps going.
//!
//! **Both are sent, and that is deliberate.** A locked application reads relative motion and
//! ignores position; an unlocked one is the opposite; a real mouse on a real compositor
//! produces both at once and applications are written expecting that. The one case where the
//! absolute half is suppressed is an *active lock* — there, moving the pointer is precisely
//! what the application asked us not to do.
//!
//! Deltas arrive in the session's logical pixels; the client does that conversion, the same one
//! it already does for scroll.

use smithay::{
    input::pointer::{MotionEvent, RelativeMotionEvent},
    utils::{Logical, Point},
    wayland::pointer_constraints::{PointerConstraint, with_pointer_constraint},
};

use crate::Wado;

impl Wado {
    /// Relative pointer motion from a pointer-locked client.
    pub(crate) fn pointer_relative(&mut self, dx: f64, dy: f64) {
        let (serial, time) = self.input_clock();
        let pointer = self.seat.get_pointer().unwrap();
        let delta: Point<f64, Logical> = (dx, dy).into();

        let from = pointer.current_location();
        let under = self.surface_under(from);
        let locked = under
            .as_ref()
            .is_some_and(|(surface, _)| self.pointer_locked_on(surface));

        // `delta_unaccel` is the same figure: acceleration is a libinput concept for a physical
        // device, and these deltas have already been through the browser's own pointer
        // acceleration before they were sent. Applying a second curve here would make a
        // configured mouse feel wrong in a way no setting could undo.
        pointer.relative_motion(
            self,
            under.clone(),
            &RelativeMotionEvent {
                delta,
                delta_unaccel: delta,
                utime: time as u64 * 1000,
            },
        );

        if locked {
            // The pointer stays where it is: that is what the lock means.
            pointer.frame(self);
            return;
        }

        let to = self.clamp_to_output(from + delta);
        let under = self.surface_under(to);
        pointer.motion(
            self,
            under.clone(),
            &MotionEvent {
                location: to,
                serial,
                time,
            },
        );
        pointer.frame(self);

        // Arriving on a surface that had asked for a lock is what activates it — the request
        // usually comes in while the pointer is somewhere else entirely.
        if let Some((surface, _)) = under {
            with_pointer_constraint(&surface, &pointer, |constraint| {
                if let Some(constraint) = constraint {
                    if !constraint.is_active() {
                        constraint.activate();
                    }
                }
            });
        }
    }

    /// Where a positional event should act while a lock is held.
    ///
    /// The browser freezes `clientX`/`clientY` at the moment of the lock, so a click or a
    /// scroll that arrives during one carries a position from before the game started turning.
    /// Acting on it would teleport the pointer — which is exactly what the lock exists to
    /// prevent — so the pointer's own current location stands in, and no motion is sent.
    ///
    /// `None` means "not locked, use the position that arrived".
    pub(crate) fn locked_pointer_location(&self) -> Option<Point<f64, Logical>> {
        let pointer = self.seat.get_pointer()?;
        let at = pointer.current_location();
        let (surface, _) = self.surface_under(at)?;
        self.pointer_locked_on(&surface).then_some(at)
    }

    /// Is an *active* pointer lock held on this surface?
    ///
    /// Confinement is deliberately not included: it asks for the pointer to be kept inside a
    /// region rather than frozen, and a region that is only ever a whole window on a
    /// single-window-per-screen session is already what `clamp_to_output` does.
    fn pointer_locked_on(
        &self,
        surface: &smithay::reexports::wayland_server::protocol::wl_surface::WlSurface,
    ) -> bool {
        let Some(pointer) = self.seat.get_pointer() else {
            return false;
        };
        with_pointer_constraint(surface, &pointer, |constraint| {
            constraint.is_some_and(|c| c.is_active() && matches!(&*c, PointerConstraint::Locked(_)))
        })
    }

    /// Keep a point on the output. Relative motion has no natural bound — a mouse pushed left
    /// for a second would otherwise put the pointer thousands of pixels off-screen, and every
    /// one of those pixels has to be travelled back before anything responds.
    fn clamp_to_output(&self, point: Point<f64, Logical>) -> Point<f64, Logical> {
        match self
            .space
            .outputs()
            .next()
            .and_then(|o| self.space.output_geometry(o))
        {
            Some(geo) => clamp_point(point, geo),
            None => point,
        }
    }
}

/// Keep `point` inside `geo`.
///
/// Half-open on the far edge: a pointer at exactly `x + w` is on the first column *past* the
/// output, which has no surface under it — so a mouse held against the right edge would report
/// nothing there rather than the window that reaches it.
fn clamp_point(
    point: Point<f64, Logical>,
    geo: smithay::utils::Rectangle<i32, Logical>,
) -> Point<f64, Logical> {
    (
        point
            .x
            .clamp(geo.loc.x as f64, (geo.loc.x + geo.size.w) as f64 - 1.0),
        point
            .y
            .clamp(geo.loc.y as f64, (geo.loc.y + geo.size.h) as f64 - 1.0),
    )
        .into()
}

#[cfg(test)]
mod tests {
    use super::clamp_point;
    use smithay::utils::{Logical, Point, Rectangle};

    fn output() -> Rectangle<i32, Logical> {
        Rectangle::new(Point::from((0, 0)), (1280, 720).into())
    }

    #[test]
    fn a_point_on_the_output_is_left_alone() {
        let p: Point<f64, Logical> = (640.0, 360.0).into();
        assert_eq!(clamp_point(p, output()), p);
    }

    #[test]
    fn a_long_push_stops_at_the_edge_instead_of_running_away() {
        // The case this exists for: a mouse dragged left for a second. Without the clamp the
        // pointer ends up thousands of pixels out and every one has to be travelled back.
        let far: Point<f64, Logical> = (-9000.0, -9000.0).into();
        assert_eq!(clamp_point(far, output()), (0.0, 0.0).into());
        let far: Point<f64, Logical> = (9000.0, 9000.0).into();
        assert_eq!(clamp_point(far, output()), (1279.0, 719.0).into());
    }

    #[test]
    fn the_far_edge_is_the_last_real_pixel() {
        // Exactly the output size is one past the end — there is no surface there.
        let edge: Point<f64, Logical> = (1280.0, 720.0).into();
        assert_eq!(clamp_point(edge, output()), (1279.0, 719.0).into());
    }

    #[test]
    fn an_output_not_at_the_origin_is_respected() {
        let geo = Rectangle::new(Point::from((100, 50)), (200, 100).into());
        assert_eq!(clamp_point((0.0, 0.0).into(), geo), (100.0, 50.0).into());
        assert_eq!(
            clamp_point((9000.0, 9000.0).into(), geo),
            (299.0, 149.0).into()
        );
    }
}
