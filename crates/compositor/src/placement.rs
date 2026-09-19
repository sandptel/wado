//! New-window placement: where freshly-mapped toplevels land on the output, per the
//! session's [`Placement`] setting.
//!
//! `TopLeft`/`Maximized` can be applied immediately at map time; `Center`/`Cascade` need the
//! window's real size, which only exists after the first commit, so those are mapped
//! provisionally and repositioned from the commit hook ([`Wado::apply_pending_placement`]).
//!
//! **Every placement goes through the commit hook now, including the two that do not need it
//! for position.** The reason is [`crate::fit`]: whether a window is too big for the output is
//! also a question that cannot be answered before the first commit, and it has to be asked of
//! a top-left window exactly as much as of a centred one.

use smithay::{
    desktop::Window,
    reexports::wayland_protocols::xdg::shell::server::xdg_toplevel,
    reexports::wayland_server::protocol::wl_surface::WlSurface,
    utils::{IsAlive, Logical, Point},
};

use wado_protocol::Placement;

use crate::{Wado, fit};

/// Pixels between cascaded windows, and how many steps before wrapping back.
const CASCADE_STEP: i32 = 32;
const CASCADE_WRAP: u32 = 8;

impl Wado {
    /// Map a new toplevel according to [`Wado::placement`]. Called from `new_toplevel`.
    pub(crate) fn place_new_toplevel(&mut self, window: Window) {
        let output_geo = self
            .space
            .outputs()
            .next()
            .and_then(|o| self.space.output_geometry(o));

        // Before anything else, and before the initial configure goes out: this is the one
        // message that can stop an application picking a desktop-sized default in the first
        // place, and it is only useful if the client reads it while deciding.
        if let (Some(geo), Some(toplevel)) = (output_geo, window.toplevel()) {
            fit::advertise_bounds(toplevel, geo.size);
        }

        match self.placement {
            Placement::Maximized => {
                if let Some(geo) = output_geo {
                    let xdg = window.toplevel().unwrap();
                    // The state, not only the size. Apps draw differently when they know they
                    // are maximized, and it is also what `WindowAction::Maximize` reads to
                    // decide whether its toggle should maximize or restore — without it, the
                    // first press on a start-maximized window would maximize it again.
                    xdg.with_pending_state(|s| {
                        s.size = Some(geo.size);
                        s.states.set(xdg_toplevel::State::Maximized);
                    });
                    xdg.send_pending_configure();
                }
                self.space.map_element(window.clone(), (0, 0), false);
            }
            Placement::TopLeft => {
                self.space.map_element(window.clone(), (0, 0), false);
            }
            Placement::Center | Placement::Cascade => {
                // Map provisionally; reposition once the size is known (first commit).
                self.space.map_element(window.clone(), (0, 0), false);
            }
        }

        // A window whose client dies before its first sized commit never reaches
        // `apply_pending_placement`, and would sit in this list for the life of the session.
        // Cheap to sweep here, where the list is already being touched.
        self.pending_placement.retain(|w| w.alive());
        self.pending_placement.push(window);
    }

    /// Size and position a pending window once it has a real size. Called from the commit
    /// hook; a no-op for surfaces that aren't pending or still lack a size.
    ///
    /// Runs exactly once per window, and that is deliberate: the fit configure below is a
    /// request the client may refuse, and re-asking on every commit would be an endless
    /// configure loop against any application whose minimum size is larger than the output.
    pub(crate) fn apply_pending_placement(&mut self, surface: &WlSurface) {
        let Some(idx) = self
            .pending_placement
            .iter()
            .position(|w| w.toplevel().unwrap().wl_surface() == surface)
        else {
            return;
        };
        let window = self.pending_placement[idx].clone();
        if window.geometry().size.is_empty() {
            return; // wait for a real size
        }
        let Some(output_geo) = self
            .space
            .outputs()
            .next()
            .and_then(|o| self.space.output_geometry(o))
        else {
            return;
        };

        // Shrink first, place second — placing by the pre-shrink size would centre a window
        // by dimensions it is about to stop having.
        let size = fit::fit(&window, output_geo.size);

        let loc: Point<i32, Logical> = match self.placement {
            Placement::Center => {
                let x = output_geo.loc.x + (output_geo.size.w - size.w).max(0) / 2;
                let y = output_geo.loc.y + (output_geo.size.h - size.h).max(0) / 2;
                (x, y).into()
            }
            Placement::Cascade => {
                let n = (self.cascade_count % CASCADE_WRAP) as i32;
                self.cascade_count = self.cascade_count.wrapping_add(1);
                // Clamped, so a cascade step never pushes a full-width window off the right
                // edge — the step is decoration, the window being reachable is not.
                let x = (output_geo.loc.x + n * CASCADE_STEP)
                    .min(output_geo.loc.x + (output_geo.size.w - size.w).max(0));
                let y = (output_geo.loc.y + n * CASCADE_STEP)
                    .min(output_geo.loc.y + (output_geo.size.h - size.h).max(0));
                (x, y).into()
            }
            // Maximized and TopLeft: the origin, and the fit above is the whole reason they
            // come through here at all.
            _ => (output_geo.loc.x, output_geo.loc.y).into(),
        };

        self.space.map_element(window, loc, false);
        self.pending_placement.remove(idx);
    }
}
