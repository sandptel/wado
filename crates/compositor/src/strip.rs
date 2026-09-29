//! The phone shell's scrolling strip (`Placement::Strip`), niri-style.
//!
//! One job: lay toplevels out as columns in one horizontal row and keep the focused column on
//! screen. The output is a viewport onto the row; columns outside it are mapped off-output,
//! which is enough to hide them — the renderer draws only what intersects the output, and
//! input only reaches what is under it.
//!
//! Column width: the whole output in portrait. In landscape a column is full or half width,
//! toggled by `WindowAction::Maximize` — "fill the screen or not" is what maximize already
//! means, so the strip reuses it instead of adding a verb. Dialogs (a toplevel with a parent)
//! are not columns: they float, centred, through the ordinary placement path.
//!
//! Decided in the Decision Log, `2026-09-29` (phone UX track, S2).

use smithay::desktop::Window;
use smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel;
use smithay::utils::{IsAlive, Size};

use crate::Wado;

/// One column of the strip.
pub struct Column {
    pub window: Window,
    /// Half the output wide instead of all of it. Only honoured in landscape.
    pub half: bool,
    /// Just inserted and not yet focused. Focus waits for the initial configure: at insert
    /// time the client has not made its first commit, and xdg-shell forbids configuring it yet.
    pub fresh: bool,
}

/// Where each column starts on the row, and the viewport offset that keeps `focused` fully
/// visible. The previous offset is kept whenever it already does, so switching between two
/// visible halves does not scroll.
pub fn layout(widths: &[i32], focused: usize, offset: i32, view_w: i32) -> (Vec<i32>, i32) {
    let mut xs = Vec::with_capacity(widths.len());
    let mut x = 0;
    for w in widths {
        xs.push(x);
        x += w;
    }
    let total = x;
    let mut offset = offset;
    if let (Some(left), Some(w)) = (xs.get(focused), widths.get(focused)) {
        let right = left + w;
        if *left < offset {
            offset = *left;
        } else if right > offset + view_w {
            offset = right - view_w;
        }
    }
    // Never scroll past the end of the row, and never before its start.
    offset = offset.min((total - view_w).max(0)).max(0);
    (xs, offset)
}

impl Wado {
    /// Whether a new toplevel becomes a column (true) or floats as a dialog (false).
    pub(crate) fn is_strip_column(window: &Window) -> bool {
        window.toplevel().is_some_and(|t| t.parent().is_none())
    }

    /// Insert a new toplevel as a column right after the focused one. It is focused — and so
    /// scrolled to — by [`Self::strip_tick`] once it has been configured.
    pub(crate) fn strip_insert(&mut self, window: Window) {
        let at = self
            .focused_window()
            .and_then(|f| self.strip.iter().position(|c| c.window == f))
            .map_or(self.strip.len(), |i| i + 1);
        self.strip.insert(
            at,
            Column {
                window,
                half: false,
                fresh: true,
            },
        );
        self.strip_relayout();
    }

    /// Post-dispatch upkeep: focus columns that have just been configured, and drop columns
    /// whose client is gone. Cheap enough to run every dispatch, which is where it runs —
    /// there is no toplevel-destroyed handler to hang it on.
    pub fn strip_tick(&mut self) {
        let ready = self.strip.iter_mut().find(|c| {
            c.fresh
                && c.window
                    .toplevel()
                    .is_some_and(|t| t.is_initial_configure_sent())
        });
        if let Some(c) = ready {
            c.fresh = false;
            let w = c.window.clone();
            self.focus_window(&w); // scrolls to it via the focus hook
        }
        self.strip_prune();
    }

    fn strip_prune(&mut self) {
        let before = self.strip.len();
        self.strip.retain(|c| c.window.alive());
        if self.strip.len() == before {
            return;
        }
        self.strip_focused = self.strip_focused.min(self.strip.len().saturating_sub(1));
        if let Some(c) = self.strip.get(self.strip_focused) {
            let w = c.window.clone();
            self.focus_window(&w); // relayouts via the focus hook
        } else {
            self.strip_offset = 0;
        }
    }

    /// Focus moved to `window`: scroll so its column is on screen.
    pub(crate) fn strip_follow_focus(&mut self, window: &Window) {
        if let Some(i) = self.strip.iter().position(|c| &c.window == window) {
            self.strip_focused = i;
            self.strip_relayout();
        }
    }

    /// Toggle the focused column between full and half width (landscape only).
    pub(crate) fn strip_toggle_width(&mut self, window: &Window) {
        if let Some(c) = self.strip.iter_mut().find(|c| &c.window == window) {
            c.half = !c.half;
        }
        self.strip_relayout();
    }

    /// Size every column and map it at its place on the row, relative to the viewport.
    pub(crate) fn strip_relayout(&mut self) {
        let Some(geo) = self
            .space
            .outputs()
            .next()
            .and_then(|o| self.space.output_geometry(o))
        else {
            return;
        };
        // Focus can move without passing through `focus_window` (a tap focuses what is under
        // it), so the index is re-derived here rather than trusted.
        if let Some(i) = self
            .focused_window()
            .and_then(|f| self.strip.iter().position(|c| c.window == f))
        {
            self.strip_focused = i;
        }
        let landscape = geo.size.w > geo.size.h;
        let widths: Vec<i32> = self
            .strip
            .iter()
            .map(|c| {
                let full = crate::fullscreen::is_fullscreen(&c.window);
                if landscape && c.half && !full {
                    geo.size.w / 2
                } else {
                    geo.size.w
                }
            })
            .collect();
        let (xs, offset) = layout(&widths, self.strip_focused, self.strip_offset, geo.size.w);
        self.strip_offset = offset;

        for ((c, x), w) in self.strip.iter().zip(&xs).zip(&widths) {
            let Some(toplevel) = c.window.toplevel() else {
                continue;
            };
            // Fullscreen already owns its size (see `crate::fullscreen`); only its place on
            // the row is the strip's business.
            if !crate::fullscreen::is_fullscreen(&c.window) {
                let size = Size::from((*w, geo.size.h));
                toplevel.with_pending_state(|s| {
                    s.size = Some(size);
                    // Maximized, so apps drop the shadows and rounded corners of a floating
                    // window — a column is edge to edge.
                    s.states.set(xdg_toplevel::State::Maximized);
                });
                // Only once the client has had its initial configure; before that, the commit
                // hook sends it and it carries this pending state.
                if toplevel.is_initial_configure_sent() {
                    toplevel.send_pending_configure();
                }
            }
            let loc = (geo.loc.x + x - offset, geo.loc.y);
            self.space.map_element(c.window.clone(), loc, false);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::layout;

    #[test]
    fn full_columns_scroll_one_screen_per_column() {
        let (xs, off) = layout(&[400, 400, 400], 2, 0, 400);
        assert_eq!(xs, [0, 400, 800]);
        assert_eq!(off, 800);
        // Back to the first: the viewport follows left too.
        assert_eq!(layout(&[400, 400, 400], 0, 800, 400).1, 0);
    }

    #[test]
    fn a_visible_column_does_not_scroll() {
        // Two halves on screen; focusing the right one keeps the view where it is.
        assert_eq!(layout(&[400, 400, 800], 1, 0, 800).1, 0);
        // The third needs the view to move so its right edge is on screen.
        assert_eq!(layout(&[400, 400, 800], 2, 0, 800).1, 800);
    }

    #[test]
    fn never_scrolls_past_either_end() {
        assert_eq!(layout(&[400], 0, 999, 800).1, 0);
        assert_eq!(layout(&[], 0, 50, 800).1, 0);
        assert_eq!(layout(&[400, 400], 5, 0, 400).1, 0); // stale focus index
    }
}
