//! How long an application took to show a window — the wait a person sees after "launch".
//!
//! `launched session application` says when a command started; nothing said when it was on
//! screen, or that it never got there. A toplevel's first commit with a buffer is the moment it
//! is shown: matched to its launch by process group (`proc::spawn` makes each launch a group
//! leader, and an app's helpers inherit the group), it logs `window shown … N ms after launch`.
//! A launch that exits without ever showing one says so when it is reaped
//! ([`reap`]).

use smithay::reexports::wayland_server::Resource;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::wayland::compositor::with_states;
use smithay::wayland::shell::xdg::XdgToplevelSurfaceData;
use tracing::{info, warn};

use crate::state::Wado;

/// Marks a toplevel surface whose first buffer has been seen, so it is timed once.
struct Shown;

/// Call on every toplevel root commit. Logs the first one that carries a buffer.
pub fn on_commit(state: &mut Wado, surface: &WlSurface) {
    let has_buffer = smithay::backend::renderer::utils::with_renderer_surface_state(surface, |s| {
        s.buffer().is_some()
    })
    .unwrap_or(false);
    if !has_buffer {
        return;
    }
    let first = with_states(surface, |s| s.data_map.insert_if_missing(|| Shown));
    if !first {
        return;
    }
    let app_id = with_states(surface, |s| {
        s.data_map
            .get::<XdgToplevelSurfaceData>()
            .and_then(|d| d.lock().ok()?.app_id.clone())
    })
    .unwrap_or_default();
    let pid = surface
        .client()
        .and_then(|c| c.get_credentials(&state.display_handle).ok())
        .map(|c| c.pid)
        .unwrap_or(0);
    // SAFETY: getpgid reads a process's group; any pid (even a stale one) is a valid argument.
    let group = if pid > 0 {
        unsafe { libc::getpgid(pid) }
    } else {
        -1
    };
    let launch = state
        .app_processes
        .iter_mut()
        .find(|l| group > 0 && l.child.id() == group as u32);
    let mut first_shown = None;
    match launch {
        Some(l) => {
            let ms = l.at.elapsed().as_millis();
            let again = l.shown;
            l.shown = true;
            if again {
                info!(app_id, pid, command = l.command, "another window shown");
            } else {
                info!(
                    app_id,
                    pid,
                    command = l.command,
                    ms,
                    "window shown — {ms} ms after launch"
                );
                first_shown = Some((l.n, l.command.clone(), ms as u64));
            }
        }
        None => info!(app_id, pid, "window shown (not launched by wado)"),
    }
    if let Some((n, command, ms)) = first_shown {
        note(state, n, &command, "shown", ms);
    }
}

static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// The next launch's number (`LaunchNote::n`).
pub fn next() -> u64 {
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1
}

/// Say how launch `n` is going to every viewer, with the window list (`WindowList::launches`):
/// `starting`, `shown` or `exited`. Only the latest few are kept.
pub fn note(state: &mut Wado, n: u64, command: &str, st: &str, ms: u64) {
    state.windows_tx.send_modify(|list| {
        let l = &mut list.launches;
        match l.iter_mut().find(|x| x.n == n) {
            Some(x) => {
                x.state = st.to_string();
                x.ms = ms;
            }
            None => l.push(wado_protocol::LaunchNote {
                n,
                command: command.to_string(),
                state: st.to_string(),
                ms,
            }),
        }
        let extra = l.len().saturating_sub(wado_protocol::LAUNCHES_KEPT);
        l.drain(..extra);
    });
}

/// Forget launches that have exited, saying so for one that never showed a window — the
/// "nothing appeared" a person sees, otherwise logged nowhere.
pub fn reap(state: &mut Wado) {
    let mut gone = Vec::new();
    state
        .app_processes
        .retain_mut(|app| match app.child.try_wait() {
            Ok(Some(status)) => {
                if !app.shown {
                    let after_ms = app.at.elapsed().as_millis() as u64;
                    warn!(
                        command = app.command,
                        %status,
                        after_ms,
                        "application exited without showing a window"
                    );
                    gone.push((app.n, app.command.clone(), after_ms));
                }
                false
            }
            _ => true,
        });
    for (n, command, ms) in gone {
        note(state, n, &command, "exited", ms);
    }
}
