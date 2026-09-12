//! `zxdg_decoration_manager_v1` — wado tells every window to draw no decorations.
//!
//! **The protocol's whole value is the answer, so a wrong answer is worse than no protocol.**
//! Without the global, toolkits assume client-side decorations and draw their own titlebar,
//! shadow and border. Replying `ClientSide` is therefore a no-op; `ServerSide` is the only
//! reply that changes anything, and since wado draws no decorations at all, it means
//! **borderless**.
//!
//! Borderless is the right answer for this compositor, not a shortcut:
//!
//! - The viewer is usually a phone. A CSD titlebar spends scarce vertical space on a strip
//!   whose close/maximize buttons are too small to hit with a finger anyway.
//! - Those buttons are already redundant — maximize, minimize, close and cycle all arrive over
//!   the remote input path (`window.rs`), driven from the client's own control bar.
//! - Dragging a window does not need a titlebar either: long-press-drag is handled by
//!   `WindowDrag` (`state.rs::WindowMove`), which never involved a titlebar in the first place.
//!
//! A client that asks for `ClientSide` is told `ServerSide` regardless. That is allowed — the
//! compositor's configure is authoritative — and it is the point: an app that insists on its own
//! titlebar would otherwise reintroduce exactly the strip this removes. GTK still keeps its
//! header bar, which is application content rather than decoration, and drops only the frame.

use smithay::reexports::wayland_protocols::xdg::decoration::zv1::server::zxdg_toplevel_decoration_v1::Mode;
use smithay::wayland::shell::xdg::ToplevelSurface;
use smithay::wayland::shell::xdg::decoration::XdgDecorationHandler;

use crate::Wado;

impl Wado {
    /// One answer, given from all three entry points below.
    fn tell_client_we_decorate(&mut self, toplevel: ToplevelSurface) {
        toplevel.with_pending_state(|state| {
            state.decoration_mode = Some(Mode::ServerSide);
        });
        // Only if the surface is already mapped: configuring an unmapped toplevel is a protocol
        // error, and `new_decoration` can arrive before the initial commit.
        if toplevel.is_initial_configure_sent() {
            toplevel.send_pending_configure();
        }
    }
}

impl XdgDecorationHandler for Wado {
    fn new_decoration(&mut self, toplevel: ToplevelSurface) {
        self.tell_client_we_decorate(toplevel);
    }

    /// The client asked for a mode. It is told the same thing either way — see the module note.
    fn request_mode(&mut self, toplevel: ToplevelSurface, _mode: Mode) {
        self.tell_client_we_decorate(toplevel);
    }

    /// The client withdrew its preference, which leaves the choice to us. Same answer.
    fn unset_mode(&mut self, toplevel: ToplevelSurface) {
        self.tell_client_we_decorate(toplevel);
    }
}
