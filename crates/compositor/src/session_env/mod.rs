//! What a launched application sees of the world outside the session.
//!
//! Separate from [`crate::proc`] on purpose: that module owns an application's *lifetime* —
//! process groups, signals, cgroup weight — and this one owns its *environment*. They are
//! applied to the same `Command` and have nothing else in common; folding them together is
//! how a spawn function grows six parameters.
//!
//! The whole reason this exists is one complaint: "I launched it in wado and it opened on my
//! computer's desktop." Setting `WAYLAND_DISPLAY` — which [`crate::build`] already does — is
//! not enough to prevent that, and the pieces that are needed are here: a private D-Bus bus
//! ([`bus`]), a `DISPLAY` that is not the host's, and — for applications that can only speak
//! X11 — a `DISPLAY` that is the session's own ([`xwayland`]).

pub mod bus;
pub mod xwayland;

use std::process::Command;

/// Where a launched application's session services come from.
///
/// Not a bool, because the isolated case has a payload and the two halves are decided in
/// different places: the *choice* comes from [`wado_protocol::SessionConfig::isolate_apps`],
/// the *address* from whether [`bus::start`] managed to start a daemon.
pub enum AppEnv {
    /// Inherit everything the daemon was started with — the pre-isolation behaviour.
    Host {
        /// The session's own X server, when it has one. It overrides the inherited `DISPLAY`
        /// even here: a session that was given an X server was given it to be used.
        x: Option<String>,
    },
    /// Keep the application away from the host desktop.
    Isolated {
        /// The private bus, when there is one. `None` means `dbus-daemon` could not be
        /// started and only the `DISPLAY` half of the isolation is in force.
        bus: Option<String>,
        /// The session's own X server, when it has one. Without it `DISPLAY` is removed and
        /// an X11-only application cannot run at all — see [`xwayland`].
        x: Option<String>,
    },
}

/// Apply the environment to a command that is about to be spawned.
pub fn apply(cmd: &mut Command, env: &AppEnv) {
    let (bus, x) = match env {
        // The host's `DISPLAY` stays as it was when there is no session X server: an X11 app
        // opening on the host desktop is wrong, but it is what "not isolated" means.
        AppEnv::Host { x } => (&None, x),
        AppEnv::Isolated { bus, x } => (bus, x),
    };

    if let Some(display) = x {
        cmd.env("DISPLAY", display);
    }

    if matches!(env, AppEnv::Host { .. }) {
        return;
    }

    // With `DISPLAY` inherited an X11 client draws on the *host's* X server and its window
    // appears over there. Chromium and Electron make this the common case rather than an edge
    // one: they pick X11 whenever `DISPLAY` is set, even with `WAYLAND_DISPLAY` present.
    //
    // Removed, they use Wayland. An X11-only application then fails where you can see it —
    // unless the session has an X server of its own, which is the branch above.
    if x.is_none() {
        cmd.env_remove("DISPLAY");
    }

    if let Some(address) = bus {
        cmd.env("DBUS_SESSION_BUS_ADDRESS", address);
        // Both point at the host's bus and would otherwise win for anything that reads them.
        cmd.env_remove("DBUS_SESSION_BUS_PID");
        cmd.env_remove("DBUS_STARTER_ADDRESS");
        cmd.env_remove("DBUS_STARTER_BUS_TYPE");
    }
}
