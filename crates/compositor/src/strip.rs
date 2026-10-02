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
//! **Apps that will not fit a column (S3).** A desktop app's minimum size is often wider than a
//! phone column (360 logical px at scale 2), and a client may refuse a configure it cannot
//! honour. Such a column gets a factor `f < 1`: the app is configured at `column / f` in its
//! own logical pixels, told the preferred scale `session scale × f` so its buffer has exactly
//! the pixels the column shows (crisp, not resampled), drawn at `× f` (see [`crate::scaled`]),
//! and touched through `÷ f` (`Wado::map_point`). It is one mechanism, not the two steps the
//! plan first described: a lower scale for one window necessarily means drawing it smaller.
//!
//! Decided in the Decision Log, `2026-09-29` (phone UX track, S2 and S3).

use smithay::desktop::Window;
use smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel;
use smithay::utils::{IsAlive, Size};
use smithay::wayland::compositor::with_states;
use smithay::wayland::fractional_scale::with_fractional_scale;
use smithay::wayland::shell::xdg::SurfaceCachedState;

use crate::Wado;

/// Logical pixels between neighbouring columns. At rest only one column is on screen, so this
/// shows only mid-slide — which is when it is needed, to tell where one window ends.
pub const GAP: i32 = 24;

/// The smallest factor a column is shrunk to. Below half size a desktop UI stops being usable
/// on a phone at all; the lens (S4) is the answer there, not a smaller picture.
pub const MIN_SCALE: f64 = 0.5;

/// The factor that makes an app fit a `col`-sized column, given what it has told us: its
/// declared minimum size, and — for apps that declare none — the size it actually committed
/// after being asked for `asked`. Only ever shrinks: an app that fits at a factor keeps it,
/// rather than oscillating as it redraws.
///
/// ponytail: monotone per window; growing back when an app's minimum shrinks is the upgrade
/// path if an app ever needs it.
pub fn fit_scale(
    col: (i32, i32),
    min: (i32, i32),
    committed: (i32, i32),
    asked: (i32, i32),
    current: f64,
) -> f64 {
    let mut f = current;
    let need = |col: i32, size: i32| f64::from(col) / f64::from(size);
    if min.0 > col.0 {
        f = f.min(need(col.0, min.0));
    }
    if min.1 > col.1 {
        f = f.min(need(col.1, min.1));
    }
    // Refused: bigger than it was asked to be, by more than rounding.
    if committed.0 > asked.0 + 1 {
        f = f.min(current * need(asked.0, committed.0));
    }
    if committed.1 > asked.1 + 1 {
        f = f.min(current * need(asked.1, committed.1));
    }
    f.clamp(MIN_SCALE, 1.0)
}

/// One column of the strip.
pub struct Column {
    pub window: Window,
    /// Half the output wide instead of all of it. Only honoured in landscape.
    pub half: bool,
    /// Drawn at this factor of its own size — see the module docs. 1.0 for an app that fits.
    pub scale: f64,
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
        x += w + GAP;
    }
    let total = (x - GAP).max(0);
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

/// The viewport offset for a fractional column position — the switcher dial's view. Linear
/// between column starts, and continued past either end at the neighbouring step, so the dial's
/// rubber-band shows as the row pulling away from the edge.
pub fn view_offset(xs: &[i32], widths: &[i32], pos: f64) -> i32 {
    let (Some(&last_x), Some(&last_w)) = (xs.last(), widths.last()) else {
        return 0;
    };
    let n = xs.len();
    let i = pos.floor();
    let x_at = |k: f64| -> f64 {
        if k < 0.0 {
            k * f64::from(widths[0] + GAP)
        } else if k as usize >= n {
            f64::from(last_x) + (k - (n - 1) as f64) * f64::from(last_w + GAP)
        } else {
            f64::from(xs[k as usize])
        }
    };
    let (a, b) = (x_at(i), x_at(i + 1.0));
    (a + (b - a) * (pos - i)).round() as i32
}

impl Wado {
    /// The switcher dial is driving the viewport (`Some`), or hands it back (`None`).
    pub(crate) fn strip_view(&mut self, pos: Option<f64>) {
        if self.placement != wado_protocol::Placement::Strip {
            return;
        }
        self.strip_view = pos.filter(|p| p.is_finite());
        self.strip_relayout();
    }

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
                scale: 1.0,
                fresh: true,
            },
        );
        self.strip_relayout();
    }

    /// Post-dispatch upkeep: focus columns that have just been configured, and drop columns
    /// whose client is gone. Cheap enough to run every dispatch, which is where it runs —
    /// there is no toplevel-destroyed handler to hang it on.
    /// Each column's on-screen width: the output's, or half of it for a half column in
    /// landscape. Fullscreen is always the whole width.
    fn strip_widths(&self, out: Size<i32, smithay::utils::Logical>) -> Vec<i32> {
        let landscape = out.w > out.h;
        self.strip
            .iter()
            .map(|c| {
                let full = crate::fullscreen::is_fullscreen(&c.window);
                if landscape && c.half && !full {
                    out.w / 2
                } else {
                    out.w
                }
            })
            .collect()
    }

    /// Shrink any column whose app will not fit it (S3), and re-lay the row if one changed.
    fn strip_refit(&mut self) {
        let Some(geo) = self
            .space
            .outputs()
            .next()
            .and_then(|o| self.space.output_geometry(o))
        else {
            return;
        };
        let widths = self.strip_widths(geo.size);
        let session_scale = f64::from(self.output_scale);
        let mut changed = false;
        for (c, w) in self.strip.iter_mut().zip(widths) {
            let Some(t) = c.window.toplevel() else {
                continue;
            };
            if crate::fullscreen::is_fullscreen(&c.window) || !t.is_initial_configure_sent() {
                continue;
            }
            let col = (w, geo.size.h);
            let asked = (
                (f64::from(col.0) / c.scale).round() as i32,
                (f64::from(col.1) / c.scale).round() as i32,
            );
            let min = with_states(t.wl_surface(), |s| {
                s.cached_state
                    .get::<SurfaceCachedState>()
                    .current()
                    .min_size
            });
            // A refusal only counts once the app has acked what we last asked; before that its
            // committed size answers an older configure.
            let acked = with_states(t.wl_surface(), |s| {
                s.data_map
                    .get::<smithay::wayland::shell::xdg::XdgToplevelSurfaceData>()
                    .and_then(|d| d.lock().ok()?.last_acked.as_ref().map(|c| c.state.size))
            }) == Some(Some(Size::from(asked)));
            let geo_size = c.window.geometry().size;
            let committed = if acked {
                (geo_size.w, geo_size.h)
            } else {
                (0, 0)
            };
            let f = fit_scale(col, (min.w, min.h), committed, asked, c.scale);
            if (f - c.scale).abs() > 1e-3 {
                tracing::info!(
                    app_min = ?(min.w, min.h), committed = ?committed, column = ?col,
                    from = c.scale, to = f,
                    "strip: app does not fit its column — drawing it smaller"
                );
                c.scale = f;
                // Crisp rather than resampled: its buffer then has the column's own pixels.
                with_states(t.wl_surface(), |s| {
                    with_fractional_scale(s, |fs| fs.set_preferred_scale(session_scale * f))
                });
                changed = true;
            }
        }
        if changed {
            self.strip_relayout();
        }
    }

    /// A column's draw factor; 1.0 for anything that is not a shrunk column.
    pub fn window_scale(&self, window: &smithay::desktop::Window) -> f64 {
        self.strip
            .iter()
            .find(|c| &c.window == window)
            .map_or(1.0, |c| c.scale)
    }

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
        self.strip_refit();
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

    /// Minimize, for a row of columns: send `window` to the end of the row and show the column
    /// that was beside it. Reversible and still reachable from the dial, which is what
    /// minimizing means when nothing can be hidden behind anything else.
    pub(crate) fn strip_send_back(&mut self, window: &Window) {
        let Some(i) = self.strip.iter().position(|c| &c.window == window) else {
            return;
        };
        if self.strip.len() < 2 {
            return;
        }
        let col = self.strip.remove(i);
        self.strip.push(col);
        // The neighbour that slid into its place, or the one before when it was already last.
        let next = self.strip[i.min(self.strip.len() - 2)].window.clone();
        self.focus_window(&next); // relayouts via the focus hook
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
        let widths = self.strip_widths(geo.size);
        let (xs, offset) = layout(&widths, self.strip_focused, self.strip_offset, geo.size.w);
        // The dial's view wins while it is held or springing; focus-following resumes from
        // wherever it leaves the row, which is the focused column once it has settled.
        let offset = match self.strip_view {
            Some(pos) => view_offset(&xs, &widths, pos),
            None => offset,
        };
        self.strip_offset = offset;

        for ((c, x), w) in self.strip.iter().zip(&xs).zip(&widths) {
            let Some(toplevel) = c.window.toplevel() else {
                continue;
            };
            // Home put it away; the strip keeps its place but does not show it.
            if self.is_parked(&c.window) {
                continue;
            }
            // Fullscreen already owns its size (see `crate::fullscreen`); only its place on
            // the row is the strip's business.
            if !crate::fullscreen::is_fullscreen(&c.window) {
                // In the app's own logical pixels: a column drawn at `× scale` must be asked
                // for `÷ scale` to fill it.
                let size = Size::from((
                    (f64::from(*w) / c.scale).round() as i32,
                    (f64::from(geo.size.h) / c.scale).round() as i32,
                ));
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
    use super::{GAP, MIN_SCALE, fit_scale, layout};

    #[test]
    fn an_app_that_fits_is_left_alone() {
        assert_eq!(
            fit_scale((360, 700), (200, 300), (360, 700), (360, 700), 1.0),
            1.0
        );
    }

    #[test]
    fn a_declared_minimum_wider_than_the_column_shrinks_it() {
        // nautilus-like: min 380 wide in a 360 column.
        let f = fit_scale((360, 700), (380, 0), (0, 0), (360, 700), 1.0);
        assert!((f - 360.0 / 380.0).abs() < 1e-9);
    }

    #[test]
    fn a_refusal_without_a_declared_minimum_shrinks_it_too() {
        // Asked for 360, drew 480: needs 0.75 to fit.
        let f = fit_scale((360, 700), (0, 0), (480, 700), (360, 700), 1.0);
        assert!((f - 0.75).abs() < 1e-9);
        // Already at 0.75 and asked 480, it drew 480: fits, stays.
        assert_eq!(
            fit_scale((360, 700), (0, 0), (480, 700), (480, 933), 0.75),
            0.75
        );
    }

    #[test]
    fn never_below_the_floor_and_never_grows() {
        assert_eq!(
            fit_scale((360, 700), (2000, 0), (0, 0), (360, 700), 1.0),
            MIN_SCALE
        );
        assert_eq!(fit_scale((360, 700), (0, 0), (0, 0), (360, 700), 0.8), 0.8);
    }

    #[test]
    fn full_columns_scroll_one_screen_per_column() {
        let (xs, off) = layout(&[400, 400, 400], 2, 0, 400);
        assert_eq!(xs, [0, 400 + GAP, 800 + 2 * GAP]);
        assert_eq!(off, 800 + 2 * GAP);
        // Back to the first: the viewport follows left too.
        assert_eq!(layout(&[400, 400, 400], 0, 800, 400).1, 0);
    }

    #[test]
    fn a_visible_column_does_not_scroll() {
        // Two halves (with the gap) on a view wide enough for both: no scroll.
        assert_eq!(layout(&[400, 400, 800], 1, 0, 800 + GAP).1, 0);
        // The third needs the view to move so its right edge is on screen.
        assert_eq!(layout(&[400, 400, 800], 2, 0, 800).1, 800 + 2 * GAP);
    }

    #[test]
    fn the_dial_view_is_linear_between_columns_and_runs_past_the_ends() {
        use super::view_offset;
        let (xs, w) = (vec![0, 400 + GAP], vec![400, 400]);
        assert_eq!(view_offset(&xs, &w, 0.0), 0);
        assert_eq!(view_offset(&xs, &w, 1.0), 400 + GAP);
        assert_eq!(view_offset(&xs, &w, 0.5), (400 + GAP) / 2);
        assert!(
            view_offset(&xs, &w, -0.25) < 0,
            "rubber-band before the first"
        );
        assert!(view_offset(&xs, &w, 1.25) > 400 + GAP, "and after the last");
        // Settling on a column lands exactly where focus-following would put it: no jump.
        assert_eq!(view_offset(&xs, &w, 1.0), layout(&w, 1, 0, 400).1);
    }

    #[test]
    fn never_scrolls_past_either_end() {
        assert_eq!(layout(&[400], 0, 999, 800).1, 0);
        assert_eq!(layout(&[], 0, 50, 800).1, 0);
        assert_eq!(layout(&[400, 400], 5, 0, 400).1, 0); // stale focus index
    }
}
