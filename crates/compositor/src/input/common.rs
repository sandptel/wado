//! Shared plumbing for the per-feature input synthesizers (`keyboard`, `touch`, `pointer`,
//! `window_drag`): the input clock (serial + timestamp), normalized→logical coordinate
//! mapping, and click/tap-to-focus. Kept in one place so each feature module stays focused
//! on its own Wayland synthesis.

use smithay::utils::{Logical, Point, SERIAL_COUNTER, Serial};

use crate::Wado;

impl Wado {
    /// A fresh `(serial, time_ms)` pair for one synthesized input event.
    pub(crate) fn input_clock(&self) -> (Serial, u32) {
        (
            SERIAL_COUNTER.next_serial(),
            self.start_time.elapsed().as_millis() as u32,
        )
    }

    /// Map normalized 0..1 client coordinates to a point in the output's logical space.
    /// `None` if there is no mapped output yet.
    pub(crate) fn map_point(&self, x: f64, y: f64) -> Option<Point<f64, Logical>> {
        let output = self.space.outputs().next()?;
        let geo = self.space.output_geometry(output)?;
        let lx = geo.loc.x as f64 + x.clamp(0.0, 1.0) * geo.size.w as f64;
        let ly = geo.loc.y as f64 + y.clamp(0.0, 1.0) * geo.size.h as f64;
        Some(self.unshrink((lx, ly).into()))
    }

    /// A point on a shrunk strip column (S3), mapped into the space's coordinates for that
    /// window: it is drawn at `× f` about its location, so a point on it is `÷ f` from there.
    /// Everything downstream — hit testing, surface-local coordinates, grabs — then works
    /// unchanged. Points anywhere else pass through.
    fn unshrink(&self, p: Point<f64, Logical>) -> Point<f64, Logical> {
        for c in self.strip.iter().filter(|c| c.scale < 1.0) {
            let Some(loc) = self.space.element_location(&c.window) else {
                continue;
            };
            let loc = loc.to_f64();
            let size = c.window.geometry().size.to_f64().upscale(c.scale);
            let inside =
                p.x >= loc.x && p.y >= loc.y && p.x < loc.x + size.w && p.y < loc.y + size.h;
            if inside {
                return loc + (p - loc).downscale(c.scale);
            }
        }
        p
    }

    /// Raise the window under `loc` and give it keyboard focus (tap/click-to-focus).
    pub(crate) fn focus_window_at(&mut self, loc: Point<f64, Logical>, serial: Serial) {
        if let Some((window, _)) = self.space.element_under(loc).map(|(w, l)| (w.clone(), l)) {
            self.space.raise_element(&window, true);
            let keyboard = self.seat.get_keyboard().unwrap();
            keyboard.set_focus(
                self,
                Some(window.toplevel().unwrap().wl_surface().clone()),
                serial,
            );
            self.space.elements().for_each(|w| {
                w.toplevel().unwrap().send_pending_configure();
            });
        }
    }
}
