//! New-window placement: where freshly-mapped toplevels land on the output, per the
//! session's [`Placement`] setting.
//!
//! `TopLeft`/`Maximized` can be applied immediately at map time; `Center`/`Cascade` need the
//! window's real size, which only exists after the first commit, so those are mapped
//! provisionally and repositioned from the commit hook ([`Wado::apply_pending_placement`]).

use smithay::{
    desktop::Window,
    reexports::wayland_server::protocol::wl_surface::WlSurface,
    utils::{Logical, Point},
};

use wado_protocol::Placement;

use crate::Wado;

/// Pixels between cascaded windows, and how many steps before wrapping back.
const CASCADE_STEP: i32 = 32;
const CASCADE_WRAP: u32 = 8;

impl Wado {
    /// Map a new toplevel according to [`Wado::placement`]. Called from `new_toplevel`.
    pub(crate) fn place_new_toplevel(&mut self, window: Window) {
        let output_geo =
            self.space.outputs().next().and_then(|o| self.space.output_geometry(o));
        match self.placement {
            Placement::Maximized => {
                if let Some(geo) = output_geo {
                    let xdg = window.toplevel().unwrap();
                    xdg.with_pending_state(|s| s.size = Some(geo.size));
                    xdg.send_pending_configure();
                }
                self.space.map_element(window, (0, 0), false);
            }
            Placement::TopLeft => {
                self.space.map_element(window, (0, 0), false);
            }
            Placement::Center | Placement::Cascade => {
                // Map provisionally; reposition once the size is known (first commit).
                self.space.map_element(window.clone(), (0, 0), false);
                self.pending_placement.push(window);
            }
        }
    }

    /// Reposition a pending Center/Cascade window once it has a real size. Called from the
    /// commit hook; a no-op for surfaces that aren't pending or still lack a size.
    pub(crate) fn apply_pending_placement(&mut self, surface: &WlSurface) {
        let Some(idx) = self
            .pending_placement
            .iter()
            .position(|w| w.toplevel().unwrap().wl_surface() == surface)
        else {
            return;
        };
        let window = self.pending_placement[idx].clone();
        let size = window.geometry().size;
        if size.w == 0 || size.h == 0 {
            return; // wait for a real size
        }
        let Some(output_geo) =
            self.space.outputs().next().and_then(|o| self.space.output_geometry(o))
        else {
            return;
        };

        let loc: Point<i32, Logical> = match self.placement {
            Placement::Center => {
                let x = output_geo.loc.x + (output_geo.size.w - size.w).max(0) / 2;
                let y = output_geo.loc.y + (output_geo.size.h - size.h).max(0) / 2;
                (x, y).into()
            }
            Placement::Cascade => {
                let n = (self.cascade_count % CASCADE_WRAP) as i32;
                self.cascade_count = self.cascade_count.wrapping_add(1);
                (output_geo.loc.x + n * CASCADE_STEP, output_geo.loc.y + n * CASCADE_STEP).into()
            }
            _ => (0, 0).into(),
        };

        self.space.map_element(window, loc, false);
        self.pending_placement.remove(idx);
    }
}
