//! A D-Bus session bus that belongs to the wado session, not to the host desktop.
//!
//! **The problem it solves.** Launching an application into the session already sets
//! `WAYLAND_DISPLAY`, which is enough for something starting from scratch. It is not enough
//! for the common case: a single-instance application — a browser, a file manager, most GTK
//! apps — first asks the *session bus* whether a copy of itself is already running. On a
//! developer's desktop one usually is, so the new process hands its command line to the old
//! one and exits, and a window opens on the host compositor. From inside wado that looks like
//! "launching Firefox did nothing", which is exactly how it was reported.
//!
//! A private bus breaks that handshake in the only place it can be broken: the application
//! looks, finds nothing, and starts for real inside the session.
//!
//! **What is given up.** The bus is empty. Nothing session-scoped that the host desktop
//! provides is reachable from it — no notification daemon, no XDG desktop portals (so a GTK
//! file chooser falls back to its own), no secrets service, no media keys. That is the point
//! of the isolation, and it is the cost of it. Audio is unaffected: PipeWire and PulseAudio
//! are reached through sockets in `XDG_RUNTIME_DIR`, which is deliberately left alone.
//!
//! ponytail: `dbus-daemon` as a child process, one line of stdout read for the address. The
//! alternative — `dbus-run-session` — wraps a single command and would give every application
//! its own bus, so two applications in the same session could not see each other either.

use std::{
    io::{BufRead, BufReader},
    process::{Child, Command, Stdio},
    sync::mpsc,
    time::Duration,
};

use tracing::{info, warn};

/// How long to wait for the daemon to print its address before giving up on it.
///
/// This runs on the calloop thread during session start, so it is a real pause in the event
/// loop. `dbus-daemon` prints the address as soon as it has bound the socket — in practice a
/// few milliseconds — and anything slower than this is a broken installation, not a slow one.
const ADDRESS_TIMEOUT: Duration = Duration::from_secs(2);

/// A running private session bus. Killed with the session that owns it.
pub struct SessionBus {
    child: Child,
    /// The value for `DBUS_SESSION_BUS_ADDRESS`.
    pub address: String,
}

/// Start a private session bus, or `None` when this machine cannot provide one.
///
/// `None` is not fatal and not silent: the session still runs and applications still launch,
/// they just share the host's bus as they did before. The log line is how that is noticed.
pub fn start() -> Option<SessionBus> {
    let mut child = match Command::new("dbus-daemon")
        .args(["--session", "--print-address", "--nofork"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(e) => {
            warn!("no private D-Bus session ({e}) — apps share the host's bus and may open windows on the host desktop");
            return None;
        }
    };

    // Read the address off a helper thread. A blocking read on the calloop thread would hang
    // the whole compositor if the daemon ever started without printing anything.
    let Some(stdout) = child.stdout.take() else {
        let _ = child.kill();
        return None;
    };
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut line = String::new();
        let read = BufReader::new(stdout).read_line(&mut line).is_ok();
        let _ = tx.send(read.then_some(line));
    });

    match rx.recv_timeout(ADDRESS_TIMEOUT) {
        Ok(Some(line)) if !line.trim().is_empty() => {
            let address = line.trim().to_string();
            info!(pid = child.id(), %address, "private D-Bus session bus for this wado session");
            Some(SessionBus { child, address })
        }
        other => {
            warn!(
                timed_out = other.is_err(),
                "dbus-daemon gave no address — apps share the host's bus"
            );
            let _ = child.kill();
            let _ = child.wait();
            None
        }
    }
}

/// Stop the bus and reap it.
///
/// SIGKILL rather than SIGTERM: the bus has no state worth flushing, and by the time this
/// runs every application that was using it has already been terminated.
pub fn terminate(mut bus: SessionBus) {
    let _ = bus.child.kill();
    let _ = bus.child.wait();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The one thing that can silently go wrong: a daemon that starts but whose address we
    /// misread, leaving every application pointed at a bus that does not exist. A real
    /// `dbus-daemon` is the only thing that proves the parse.
    #[test]
    fn the_address_is_read_and_the_socket_it_names_exists() {
        let Some(bus) = start() else {
            eprintln!("no dbus-daemon here — skipping");
            return;
        };
        assert!(
            bus.address.starts_with("unix:"),
            "unexpected address {:?}",
            bus.address
        );
        // `unix:path=/tmp/dbus-XXXX,guid=...` — the socket has to be on disk, or the address
        // parsed into something that merely looks right.
        let path = bus
            .address
            .split(',')
            .find_map(|f| f.strip_prefix("unix:path=").or(f.strip_prefix("path=")))
            .expect("address should name a socket path");
        assert!(std::path::Path::new(path).exists(), "no socket at {path}");

        terminate(bus);
    }
}
