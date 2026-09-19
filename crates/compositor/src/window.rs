//! Acting on the focused window: maximize, minimize, close, cycle focus.
//!
//! These exist because a phone has no keyboard shortcuts. On a desktop compositor every one of
//! them is a key combination; here they arrive as a [`wado_protocol::WindowAction`] from the
//! client's control bar.
//!
//! **Focused-window only, by design.** No window list is tracked or published, so nothing has
//! to be serialised or kept in sync across two transports. The consequence is named in the
//! protocol docs: `CycleFocus` is a blind rotation.
//!
//! Everything here routes through [`Wado::focused_window`] and [`Wado::focus_window`]. Doing
//! the focus change by hand at each call site is how you end up with a raised window that the
//! keyboard does not follow, or a window still drawn as active after focus left it.

use smithay::{
    desktop::Window,
    reexports::wayland_protocols::xdg::shell::server::xdg_toplevel,
    utils::{Logical, Point, Size},
};
use wado_protocol::WindowAction;

use crate::Wado;

impl Wado {
    /// The window that currently holds keyboard focus, if any.
    ///
    /// Cloned because every caller goes on to mutate `space`, which borrows it.
    pub(crate) fn focused_window(&self) -> Option<Window> {
        let surface = self.seat.get_keyboard()?.current_focus()?;
        self.space
            .elements()
            .find(|w| w.toplevel().is_some_and(|t| *t.wl_surface() == surface))
            .cloned()
    }

    /// Raise `window`, give it keyboard focus, and reconfigure every window so the activated
    /// state each one draws matches reality.
    ///
    /// The configure sweep is not optional: without it the window that *lost* focus keeps
    /// rendering its active title bar, which reads as the action having failed.
    pub(crate) fn focus_window(&mut self, window: &Window) {
        let (serial, _) = self.input_clock();
        self.space.raise_element(window, true);
        if let Some(toplevel) = window.toplevel() {
            let surface = toplevel.wl_surface().clone();
            if let Some(keyboard) = self.seat.get_keyboard() {
                keyboard.set_focus(self, Some(surface), serial);
            }
        }
        self.configure_all();
    }

    /// Flush pending state to every mapped toplevel.
    fn configure_all(&mut self) {
        for w in self.space.elements() {
            if let Some(t) = w.toplevel() {
                t.send_pending_configure();
            }
        }
    }

    /// Run one window action against the focused window.
    ///
    /// A no-op is logged rather than silent: on a phone a button that does nothing looks
    /// exactly like a dropped connection, and the log is the only place to tell them apart.
    pub(crate) fn window_action(&mut self, action: WindowAction) {
        if action == WindowAction::CycleFocus {
            return self.cycle_focus();
        }
        let Some(window) = self.focused_window() else {
            tracing::warn!(?action, "window action ignored — nothing is focused");
            return;
        };
        match action {
            WindowAction::Maximize => self.toggle_maximize(&window),
            WindowAction::Minimize => self.lower(&window),
            WindowAction::Close => {
                if let Some(t) = window.toplevel() {
                    // A request, not a kill: the app may prompt to save, or ignore it.
                    t.send_close();
                }
            }
            WindowAction::CycleFocus => unreachable!("handled above"),
        }
    }

    /// Fill the output, or go back to where the window was before.
    ///
    /// The `Maximized` state is set alongside the size, not just the size — apps draw
    /// differently when they know they are maximized (GTK header bars, dropped rounded
    /// corners), and it is also what tells us which way this toggle should go.
    fn toggle_maximize(&mut self, window: &Window) {
        let Some(toplevel) = window.toplevel() else {
            return;
        };
        // Pending rather than committed state: this toggle is the only thing that sets the
        // flag, so pending is both authoritative and immediately correct — reading committed
        // state would miss a maximize whose configure the client has not acked yet, and two
        // fast presses would then maximize twice instead of maximizing and restoring.
        let is_max =
            toplevel.with_pending_state(|s| s.states.contains(xdg_toplevel::State::Maximized));

        let output_geo = self
            .space
            .outputs()
            .next()
            .and_then(|o| self.space.output_geometry(o));

        if is_max {
            // Restore. A window with no remembered geometry (maximized at map time by the
            // session's Placement setting) gets `size = None`, which hands the choice back to
            // the client rather than inventing one.
            //
            // The remembered size is re-fitted rather than trusted: it was measured against
            // whatever the output was at the time, and a reconfigure since — a scale change is
            // enough — can have left it bigger than the screen it is being restored onto.
            let restore = self.pre_maximize.remove(window);
            toplevel.with_pending_state(|s| {
                s.size = restore.map(|(_, size)| match output_geo {
                    Some(geo) => crate::fit::shrink_to(size, geo.size).unwrap_or(size),
                    None => size,
                });
                s.states.unset(xdg_toplevel::State::Maximized);
            });
            toplevel.send_pending_configure();
            let loc = restore.map(|(loc, _)| loc).unwrap_or_default();
            self.space.map_element(window.clone(), loc, true);
            return;
        }

        let Some(geo) = output_geo else {
            tracing::warn!("maximize ignored — no output");
            return;
        };
        // Remember where it was first; after the configure the old geometry is gone.
        if let Some(loc) = self.space.element_location(window) {
            self.pre_maximize
                .insert(window.clone(), (loc, window.geometry().size));
        }
        toplevel.with_pending_state(|s| {
            s.size = Some(geo.size);
            s.states.set(xdg_toplevel::State::Maximized);
        });
        toplevel.send_pending_configure();
        self.space.map_element(window.clone(), geo.loc, true);
    }

    /// Send `window` to the bottom of the stack and focus whatever is now on top.
    ///
    /// `lower_element` takes no activate flag, so without the explicit refocus the lowered
    /// window would keep the keyboard — a window you can neither see nor stop typing into,
    /// which is precisely the unreachable state this design exists to avoid.
    fn lower(&mut self, window: &Window) {
        self.space.lower_element(window);
        match self.space.elements().last().cloned() {
            Some(top) if &top != window => self.focus_window(&top),
            // Sole window: lowering it changed nothing, so leave focus where it is rather
            // than dropping it and leaving the session with no keyboard target.
            _ => {}
        }
    }

    /// Focus and raise the next window in the stack, oldest-first, wrapping.
    fn cycle_focus(&mut self) {
        let windows: Vec<Window> = self.space.elements().cloned().collect();
        if windows.is_empty() {
            tracing::warn!("cycle focus ignored — no windows");
            return;
        }
        let next = match self.focused_window() {
            Some(current) => {
                let i = windows.iter().position(|w| *w == current).unwrap_or(0);
                windows[(i + 1) % windows.len()].clone()
            }
            // Nothing focused (an empty click, or focus was never set): start at the top.
            None => windows[windows.len() - 1].clone(),
        };
        self.focus_window(&next);
    }
}

/// Geometry remembered across a maximize, so restore has somewhere to go back to.
pub(crate) type PreMaximize = (Point<i32, Logical>, Size<i32, Logical>);
