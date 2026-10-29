//! "Start new" on a busy daemon (the user's decision, `2026-10-30`; spec §D1 P1b): a session is
//! a daemon, so a second session needs a second daemon. Find an idle one of the pool, or start a
//! sibling — capped by `server { max-sessions }` — and tell the device where to go.
//!
//! A sibling is a plain `wado daemon` with the next instance number and this daemon's
//! environment, marked `WADO_SIBLING=1`. It exits by itself when its session ends, or when no
//! session is started on it at all ([`sibling_reaper`]). When the planned supervisor lands it
//! replaces all of this.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use tracing::{info, warn};

/// Set on a daemon this module started.
pub const SIBLING_ENV: &str = "WADO_SIBLING";

/// A sibling nobody starts a session on exits after this.
const UNUSED: Duration = Duration::from_secs(120);
/// A sibling whose session ended exits after this with no new one.
const AFTER_END: Duration = Duration::from_secs(30);

/// Where a new session goes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Desk {
    /// This instance key (`2`): idle, or just started.
    Go(String),
    /// Every session this computer runs is running.
    Full { running: u32, max: u32 },
    /// A sibling could not be started — said verbatim, never retried silently.
    Failed(String),
}

/// The choice, pure: `live` daemons' instance keys, `busy` ones (a session, or a device on its
/// seat), this one's key, the sessions running and the most allowed.
pub fn choose(live: &[String], busy: &[String], me: &str, running: u32, max: u32) -> Desk {
    if !busy.iter().any(|b| b == me) {
        return Desk::Go(me.to_string());
    }
    if let Some(idle) = live.iter().find(|d| !busy.contains(d)) {
        return Desk::Go(idle.clone());
    }
    if running >= max.max(1) {
        return Desk::Full {
            running,
            max: max.max(1),
        };
    }
    Desk::Failed(String::new()) // the caller starts one
}

/// The instance keys of daemons that are busy: a session running, or a device reporting from
/// its seat (`crate::links`), so a new session never lands on someone's home page.
fn busy(remote_id: &str, me_busy: bool, me: &str) -> Vec<String> {
    let rid = wado_protocol::relay::normalize_remote_id(remote_id);
    let mut out: Vec<String> = crate::sessions::list()
        .into_iter()
        .filter_map(|s| s.instance.split_once(':').map(|(_, k)| k.to_string()))
        .collect();
    // A device reporting on a daemon: `links/<rid>_<instance>.json`.
    if let Ok(rd) = std::fs::read_dir(wado_config::paths::config_dir().join("links")) {
        for e in rd.flatten() {
            let fresh = e
                .metadata()
                .ok()
                .and_then(|m| m.modified().ok())
                .and_then(|m| m.elapsed().ok())
                .is_some_and(|age| age < Duration::from_secs(20));
            let name = e.file_name().to_string_lossy().to_string();
            if let Some(inst) = name
                .strip_prefix(&format!("{rid}_"))
                .and_then(|n| n.strip_suffix(".json"))
                && fresh
                && inst != me
            {
                out.push(inst.to_string());
            }
        }
    }
    if me_busy {
        out.push(me.to_string());
    }
    out
}

/// Find where a new session goes; start a sibling if it must. `here_busy`: this daemon runs a
/// session (or has a device on its seat).
pub async fn request(remote_id: &str, here_busy: bool) -> Desk {
    let me = crate::instance::instance_key();
    let live = crate::cli::status::daemons();
    let busy = busy(remote_id, here_busy, &me);
    let running = crate::sessions::list().len() as u32;
    let max = wado_config::live::current().server.max_sessions;
    match choose(&live, &busy, &me, running, max) {
        Desk::Failed(_) => {}
        d => return d,
    }
    let n = crate::slot::next();
    match spawn(n) {
        Ok(log) => {
            info!(instance = n, log = %log.display(), "desks: started a sibling daemon for a new session");
            // Up when its command socket answers; then a moment for its relay registration.
            for _ in 0..50 {
                if crate::cli::status::daemons().contains(&n.to_string()) {
                    tokio::time::sleep(Duration::from_millis(1500)).await;
                    return Desk::Go(n.to_string());
                }
                tokio::time::sleep(Duration::from_millis(200)).await;
            }
            Desk::Failed(format!(
                "a new session's daemon (instance {n}) did not come up — its log is {}",
                log.display()
            ))
        }
        Err(e) => {
            warn!("desks: could not start a sibling daemon: {e}");
            Desk::Failed(format!("could not start a daemon for a new session: {e}"))
        }
    }
}

/// Start `wado daemon` as instance `n`, detached, logging beside the config.
fn spawn(n: u32) -> std::io::Result<PathBuf> {
    use std::os::unix::process::CommandExt;
    let exe = std::env::current_exe()?;
    let dir = wado_config::paths::config_dir().join("logs");
    std::fs::create_dir_all(&dir)?;
    let log = dir.join(format!("daemon-{n}.log"));
    let out = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log)?;
    let err = out.try_clone()?;
    std::process::Command::new(exe)
        .arg("daemon")
        .env("WADO_INSTANCE", n.to_string())
        .env("WADO_UDP_SLICE", n.saturating_sub(1).to_string())
        .env(SIBLING_ENV, "1")
        .stdin(std::process::Stdio::null())
        .stdout(out)
        .stderr(err)
        // Its own process group: a signal for this daemon is not one for its sibling.
        .process_group(0)
        .spawn()?;
    Ok(log)
}

/// In a sibling: exit once its session has ended, or if none was ever started on it.
pub async fn sibling_reaper(session_started: Arc<AtomicBool>) {
    if std::env::var_os(SIBLING_ENV).is_none() {
        return;
    }
    let born = std::time::Instant::now();
    let mut ever = false;
    let mut idle_since: Option<std::time::Instant> = None;
    loop {
        tokio::time::sleep(Duration::from_secs(5)).await;
        let on = session_started.load(Ordering::SeqCst);
        ever |= on;
        if on {
            idle_since = None;
            continue;
        }
        let since = *idle_since.get_or_insert_with(std::time::Instant::now);
        let done = if ever {
            since.elapsed() >= AFTER_END
        } else {
            born.elapsed() >= UNUSED
        };
        if done {
            info!(
                ever_used = ever,
                "desks: this sibling daemon's session is over — exiting"
            );
            std::process::exit(0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn an_idle_daemon_starts_it_here() {
        assert_eq!(choose(&s(&["1"]), &[], "1", 0, 4), Desk::Go("1".into()));
    }

    #[test]
    fn a_busy_daemon_points_at_an_idle_sibling() {
        assert_eq!(
            choose(&s(&["1", "2", "3"]), &s(&["1", "2"]), "1", 2, 4),
            Desk::Go("3".into())
        );
    }

    #[test]
    fn none_idle_starts_one_under_the_cap() {
        assert!(matches!(
            choose(&s(&["1"]), &s(&["1"]), "1", 1, 4),
            Desk::Failed(m) if m.is_empty()
        ));
    }

    #[test]
    fn at_the_cap_it_is_full() {
        assert_eq!(
            choose(&s(&["1", "2"]), &s(&["1", "2"]), "1", 2, 2),
            Desk::Full { running: 2, max: 2 }
        );
    }
}
