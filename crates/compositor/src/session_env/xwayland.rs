//! An X server for the session, so X11-only applications run *inside* wado.
//!
//! **Why this exists.** wado speaks Wayland and nothing else. An X11-only application — Steam
//! is the one that prompted this — has no way to draw here, and before the session's
//! applications were isolated it quietly drew on the host's X server instead: the window
//! appeared on the computer running wado, not in the stream. With `DISPLAY` removed it fails
//! honestly ("Unable to open a connection to X"), which is better but still not *working*.
//!
//! **What this is.** `Xwayland` run **rootful**: an ordinary Wayland client whose single
//! window contains a whole X screen. Every X application launched into the session draws
//! inside that one window. Verified end to end on `2026-09-19` — Steam's store rendered in the
//! stream, logged in, from a frame pulled out of the encoded output.
//!
//! **Why not rootless**, which is what a desktop compositor does: rootless Xwayland requires
//! the compositor to implement the X window manager side (`-wm`, the XWM protocol), and until
//! that exists the X windows would never be mapped as Wayland surfaces at all. Rootful needs
//! nothing from the compositor — which is the whole reason it is the version that ships first.
//!
//! **The ceiling, named:** one shared X screen with no window manager inside it. X windows are
//! unmanaged — no decorations, no per-window placement, and they stack where the application
//! puts them. The upgrade path is XWM support, and it is a milestone, not a patch.
//!
//! ponytail: a child process and a display number. No smithay XWayland integration, no XWM, no
//! protocol work.

use std::{
    path::Path,
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use tracing::{info, warn};

/// Display numbers are searched from here upwards. Low numbers belong to whatever else is on
/// this machine — the host's own session is almost always `:0`.
const FIRST_DISPLAY: u32 = 20;

/// How many numbers to try before giving up. Far more sessions than this machine will ever run.
const DISPLAY_TRIES: u32 = 40;

/// How long to wait for the socket to appear before calling the start a failure.
///
/// This blocks the calloop thread during session start, like the D-Bus bus does. Xwayland is
/// ready in well under a second; anything slower is broken rather than busy.
const READY_TIMEOUT: Duration = Duration::from_secs(3);

/// A rootful X server running as a client of this session.
pub struct XServer {
    child: Child,
    /// The value for `DISPLAY`, including the colon.
    pub display: String,
}

/// Start an X server for the session, sized to the output. `None` when it could not be had.
///
/// `None` is not fatal: the session runs, and X11-only applications fail the way they did
/// before. The warning is how that is noticed.
pub fn start(width: u32, height: u32) -> Option<XServer> {
    let number = free_display()?;
    let socket = socket_path(number);

    // `-noreset` keeps the server alive between X clients: without it the server resets when
    // the last one exits, and the next application launched would find `DISPLAY` pointing at
    // nothing. Not `-fullscreen`: the window is already the size of the output, and fullscreen
    // is a state the compositor should decide, not the X server.
    let child = Command::new("Xwayland")
        .arg(format!(":{number}"))
        .args(["-geometry", &format!("{width}x{height}"), "-noreset"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        // Xwayland writes pages of xkbcomp warnings on a normal start; they are noise here and
        // a real failure is visible as the socket never appearing.
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| warn!("no X server for this session ({e}) — X11-only apps will not run"))
        .ok()?;

    let mut server = XServer {
        child,
        display: format!(":{number}"),
    };

    // The socket appearing is the only honest readiness signal. `-displayfd` reports a number
    // rather than a state, and on this machine it reported one that was already taken.
    let deadline = Instant::now() + READY_TIMEOUT;
    while Instant::now() < deadline {
        if socket.exists() {
            info!(display = %server.display, "X server for this session (rootful Xwayland)");
            return Some(server);
        }
        if matches!(server.child.try_wait(), Ok(Some(_))) {
            warn!("Xwayland exited during startup — X11-only apps will not run");
            return None;
        }
        thread::sleep(Duration::from_millis(20));
    }

    warn!("Xwayland did not come up in time — X11-only apps will not run");
    terminate(server);
    None
}

/// Stop the X server and reap it. Everything drawing into it is already gone by then.
pub fn terminate(mut server: XServer) {
    let _ = server.child.kill();
    let _ = server.child.wait();
    // Xwayland removes its own socket on a clean exit and cannot after a SIGKILL. A stale one
    // makes the next server skip that number, which is harmless — but it also makes an X client
    // connect to nothing, which is not.
    let _ = std::fs::remove_file(socket_path_str(&server.display));
}

/// The first display number nothing else is using.
fn free_display() -> Option<u32> {
    (FIRST_DISPLAY..FIRST_DISPLAY + DISPLAY_TRIES).find(|n| {
        // Both, not either: the lock outlives a crashed server, and the socket outlives one
        // that was killed. A number with either is a number that may still answer.
        !socket_path(*n).exists() && !Path::new(&format!("/tmp/.X{n}-lock")).exists()
    })
}

fn socket_path(number: u32) -> std::path::PathBuf {
    std::path::PathBuf::from(format!("/tmp/.X11-unix/X{number}"))
}

fn socket_path_str(display: &str) -> String {
    format!("/tmp/.X11-unix/X{}", display.trim_start_matches(':'))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The display number is the only thing here that can be silently wrong — a number already
    /// in use gives an X server that never starts, or worse, an application that connects to
    /// somebody else's.
    #[test]
    fn a_used_display_number_is_skipped() {
        let first = free_display().expect("some display must be free");
        assert!(first >= FIRST_DISPLAY);

        // A lock file alone is enough to disqualify a number, because that is what a crashed
        // server leaves behind.
        let lock = format!("/tmp/.X{first}-lock");
        std::fs::write(&lock, "probe").expect("write lock");
        let next = free_display();
        let _ = std::fs::remove_file(&lock);

        assert_eq!(next, Some(first + 1), "a locked number must not be offered");
    }

    /// The socket path is built twice — from a number when looking for a free one, and from a
    /// display string when cleaning up. They have to agree, or `terminate` deletes nothing.
    #[test]
    fn both_socket_paths_name_the_same_file() {
        assert_eq!(socket_path(21).to_string_lossy(), socket_path_str(":21"));
    }
}
