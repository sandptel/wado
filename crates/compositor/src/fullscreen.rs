//! Fullscreen: an application asking to own the whole output.
//!
//! Separate from [`crate::window`], which is the *viewer* acting on the focused window from the
//! bar. This is the other direction — the **application** asking, through
//! `xdg_toplevel.set_fullscreen`, which until now was silently dropped: smithay's
//! `XdgShellHandler` supplies an empty default and nothing overrode it. A game or a video
//! player asking to go fullscreen got no configure, no `Fullscreen` state, and kept its
//! windowed size, which is the one thing every one of them does on startup.
//!
//! **Why it is not just "maximize with a different flag".** Maximized means "as big as is
//! useful"; fullscreen means "the output is yours" — the client drops its decorations and its
//! header bar, and wado additionally stops drawing the focus ring over it (see
//! [`crate::glow`], and the check in `headless`'s render tick). Restore geometry is therefore
//! remembered in its own map: a window can be maximized *and* then fullscreened, and one map
//! would lose the windowed size the moment the second one happened.
//!
//! ponytail: no per-output targeting. The client may name a `wl_output` in the request and it
//! is ignored — a wado session has exactly one output by construction (invariant #8), so the
//! only honest answer to "which output?" is the one there is.

use smithay::{
    desktop::Window, reexports::wayland_protocols::xdg::shell::server::xdg_toplevel,
    wayland::shell::xdg::ToplevelSurface,
};

use crate::Wado;

/// Whether this window is currently fullscreen.
///
/// Pending rather than committed state, for the same reason [`crate::window`]'s maximize
/// toggle reads pending: it is authoritative the instant it is set, so a fullscreen whose
/// configure the client has not acked yet still reads as fullscreen — which is what the render
/// tick needs, one frame after the request rather than three.
pub fn is_fullscreen(window: &Window) -> bool {
    window.toplevel().is_some_and(|t| {
        t.with_pending_state(|s| s.states.contains(xdg_toplevel::State::Fullscreen))
    })
}

impl Wado {
    /// The mapped window backing this toplevel, if it is still mapped.
    fn window_of(&self, surface: &ToplevelSurface) -> Option<Window> {
        self.space
            .elements()
            .find(|w| w.toplevel().is_some_and(|t| t.wl_surface() == surface.wl_surface()))
            .cloned()
    }

    /// Grant or revoke fullscreen for `surface`.
    ///
    /// Always granted when there is an output: a compositor is allowed to refuse, but refusing
    /// here would mean a game that asks once at startup never gets a second chance.
    pub(crate) fn set_fullscreen(&mut self, surface: &ToplevelSurface, on: bool) {
        let Some(window) = self.window_of(surface) else {
            tracing::warn!("fullscreen request for an unmapped toplevel — ignored");
            return;
        };
        let output_geo = self
            .space
            .outputs()
            .next()
            .and_then(|o| self.space.output_geometry(o));

        if !on {
            // A window with nothing remembered gets `size = None`, handing the choice back to
            // the client rather than inventing one — the same contract as unmaximize.
            let restore = self.pre_fullscreen.remove(&window);
            surface.with_pending_state(|s| {
                s.size = restore.map(|(_, size)| match output_geo {
                    // Re-fitted, not trusted: the output may have been reconfigured while the
                    // window was fullscreen, and a remembered size can now be bigger than the
                    // screen it is going back onto.
                    Some(geo) => crate::fit::shrink_to(size, geo.size).unwrap_or(size),
                    None => size,
                });
                s.states.unset(xdg_toplevel::State::Fullscreen);
            });
            surface.send_pending_configure();
            let loc = restore.map(|(loc, _)| loc).unwrap_or_default();
            self.space.map_element(window, loc, true);
            return;
        }

        let Some(geo) = output_geo else {
            tracing::warn!("fullscreen ignored — no output");
            return;
        };
        // Remembered before the configure, because after it the windowed geometry is gone.
        // Only on the way in: a client that asks twice must not overwrite the real geometry
        // with the fullscreen one it already has.
        if let Some(loc) = self.space.element_location(&window) {
            if !self.pre_fullscreen.contains_key(&window) {
                self.pre_fullscreen
                    .insert(window.clone(), (loc, window.geometry().size));
            }
        }
        surface.with_pending_state(|s| {
            s.size = Some(geo.size);
            s.states.set(xdg_toplevel::State::Fullscreen);
        });
        surface.send_pending_configure();
        self.space.map_element(window.clone(), geo.loc, true);
        // Raise and focus: a fullscreen window underneath another one is invisible, and the
        // viewer has no window list to dig it back out with.
        self.focus_window(&window);
    }
}
