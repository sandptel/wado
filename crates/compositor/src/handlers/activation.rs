// SPDX-License-Identifier: AGPL-3.0-only
//! `xdg_activation_v1` — "raise the window I just asked for".
//!
//! The job this does here is the launcher. wado's app picker and the shell both start
//! programs, and a program started that way draws its first window *behind* whatever was
//! already focused unless something tells the compositor to raise it. Toolkits already pass
//! `XDG_ACTIVATION_TOKEN` through `exec` for exactly this, and without the global they fall
//! back to nothing — the window appears unfocused and the user taps it themselves. On a phone
//! that is a tap on a strip of window that may be entirely covered.
//!
//! Every token is honoured, which is the permissive end of the protocol. The usual reason to
//! refuse one is focus stealing between unrelated clients, and that threat model does not
//! apply: every client in a wado session was spawned by the session itself, so there is no
//! mutually-untrusting set of apps to protect from each other.

use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::wayland::xdg_activation::{
    XdgActivationHandler, XdgActivationState, XdgActivationToken, XdgActivationTokenData,
};

use crate::Wado;

impl XdgActivationHandler for Wado {
    fn activation_state(&mut self) -> &mut XdgActivationState {
        &mut self.xdg_activation_state
    }

    fn request_activation(
        &mut self,
        token: XdgActivationToken,
        token_data: XdgActivationTokenData,
        surface: WlSurface,
    ) {
        // A token can name a surface that is not a mapped toplevel in our space — a subsurface,
        // a popup, or a window that closed between the request and here. `focus_window` would
        // raise nothing and set focus to a surface with no window, so the lookup is the gate.
        let window = self
            .space
            .elements()
            .find(|w| w.toplevel().is_some_and(|t| t.wl_surface() == &surface))
            .cloned();

        match window {
            Some(window) => {
                tracing::debug!(app_id = ?token_data.app_id, "activation request — raising and focusing");
                self.focus_window(&window);
            }
            None => {
                tracing::debug!(
                    app_id = ?token_data.app_id,
                    "activation request for a surface that is not a mapped toplevel — ignored"
                );
            }
        }

        // One use per token. The protocol allows keeping it in the pool for further requests,
        // but a token that stays valid is a token that can raise a window again later, long
        // after the click that justified it.
        self.xdg_activation_state.remove_token(&token);
    }
}
