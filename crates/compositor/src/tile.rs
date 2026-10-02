//! Tiling: every window on the workspace shares the screen in a grid, re-laid whenever a window
//! comes or goes or the output changes shape. `Placement::Tile`.
//!
//! Checked on the render tick rather than hooked into every open, close and workspace switch:
//! the check is a list compare, and a hook missed is a window left floating where a layout is
//! promised. Landscape fills columns first, portrait rows — the grid follows the screen.

use smithay::{
    desktop::Window,
    reexports::wayland_protocols::xdg::shell::server::xdg_toplevel,
    utils::{Logical, Rectangle, Size},
};

use crate::Wado;

/// Space between tiles, logical pixels.
const GAP: i32 = 6;

/// The cell for window `i` of `n` in `area`.
pub fn cell(i: usize, n: usize, area: Rectangle<i32, Logical>) -> Rectangle<i32, Logical> {
    let n = n.max(1);
    let landscape = area.size.w >= area.size.h;
    let major = (n as f64).sqrt().ceil() as usize;
    let minor = n.div_ceil(major);
    let (cols, rows) = if landscape {
        (major, minor)
    } else {
        (minor, major)
    };
    let (c, r) = (i % cols, i / cols);
    // The last row stretches across when it is short, so no hole is left.
    let in_row = if r == rows - 1 {
        n - cols * (rows - 1)
    } else {
        cols
    };
    let w = (area.size.w - GAP * (in_row as i32 + 1)) / in_row as i32;
    let h = (area.size.h - GAP * (rows as i32 + 1)) / rows as i32;
    let x = area.loc.x + GAP + c as i32 * (w + GAP);
    let y = area.loc.y + GAP + r as i32 * (h + GAP);
    Rectangle::new((x, y).into(), (w.max(1), h.max(1)).into())
}

impl Wado {
    pub(crate) fn tile_tick(&mut self) {
        if self.placement != wado_protocol::Placement::Tile {
            return;
        }
        let Some(area) = self
            .space
            .outputs()
            .next()
            .and_then(|o| self.space.output_geometry(o))
        else {
            return;
        };
        // Toplevels only: dialogs float over their parent, fullscreen owns the screen.
        let tiles: Vec<Window> = self
            .space
            .elements()
            .filter(|w| w.toplevel().is_some_and(|t| t.parent().is_none()))
            .filter(|w| !crate::fullscreen::is_fullscreen(w))
            .cloned()
            .collect();
        let key = (tiles.clone(), area);
        if self.tile_last.as_ref() == Some(&key) {
            return;
        }
        let n = tiles.len();
        for (i, w) in tiles.iter().enumerate() {
            let r = cell(i, n, area);
            if let Some(t) = w.toplevel() {
                t.with_pending_state(|s| {
                    s.size = Some(Size::from((r.size.w, r.size.h)));
                    for st in [
                        xdg_toplevel::State::TiledLeft,
                        xdg_toplevel::State::TiledRight,
                        xdg_toplevel::State::TiledTop,
                        xdg_toplevel::State::TiledBottom,
                    ] {
                        s.states.set(st);
                    }
                });
                if t.is_initial_configure_sent() {
                    t.send_pending_configure();
                }
            }
            self.space.map_element(w.clone(), r.loc, false);
        }
        self.tile_last = Some(key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grids_fill_the_screen_without_holes() {
        let area = Rectangle::new((0, 0).into(), (1000, 600).into());
        let one = cell(0, 1, area);
        assert_eq!((one.size.w, one.size.h), (1000 - 2 * GAP, 600 - 2 * GAP));
        // Three in landscape: two columns, the last row stretched across.
        let a = cell(0, 3, area);
        let c = cell(2, 3, area);
        assert!(a.size.w < c.size.w, "the lone last tile spans the row");
        // Portrait stacks.
        let tall = Rectangle::new((0, 0).into(), (600, 1000).into());
        let (t0, t1) = (cell(0, 2, tall), cell(1, 2, tall));
        assert_eq!(t0.loc.x, t1.loc.x);
        assert!(t1.loc.y > t0.loc.y);
    }
}
