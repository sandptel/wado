//! The pool's running sessions, as files: `~/.config/wado/sessions/<instance>.json`.
//!
//! Each daemon writes its own while it has a session — every few seconds, so the file's age is
//! its heartbeat — and removes it when the session ends. Any daemon can then list all of them,
//! which is what lets a device landing on daemon 2 see, and get back to, the session on daemon 1.
//! Files, like the approval requests (`crate::gate`): the daemons share one machine and one
//! config directory, and the relay is kept out of anything that is not routing.

use std::{path::PathBuf, time::Duration};

use wado_protocol::SessionSummary;

/// A file not rewritten for this long belongs to a daemon that died with its session.
const STALE: Duration = Duration::from_secs(20);

fn dir() -> PathBuf {
    wado_config::paths::config_dir().join("sessions")
}

fn file(instance: &str) -> Option<PathBuf> {
    let safe = !instance.is_empty()
        && instance
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_:.".contains(c));
    safe.then(|| dir().join(format!("{}.json", instance.replace(':', "_"))))
}

pub fn write(s: &SessionSummary) {
    let Some(path) = file(&s.instance) else {
        return;
    };
    let _ = std::fs::create_dir_all(dir());
    let tmp = path.with_extension("tmp");
    if let Ok(json) = serde_json::to_vec(s) {
        if std::fs::write(&tmp, json).is_ok() {
            let _ = std::fs::rename(&tmp, &path);
        }
    }
}

pub fn clear(instance: &str) {
    if let Some(path) = file(instance) {
        let _ = std::fs::remove_file(path);
    }
}

/// Every live session, newest first. Stale files are removed on the way.
pub fn list() -> Vec<SessionSummary> {
    let Ok(rd) = std::fs::read_dir(dir()) else {
        return Vec::new();
    };
    let mut out: Vec<SessionSummary> = rd
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
        .filter_map(|e| {
            let fresh = e.metadata().ok()?.modified().ok()?.elapsed().ok()? < STALE;
            if !fresh {
                let _ = std::fs::remove_file(e.path());
                return None;
            }
            serde_json::from_slice(&std::fs::read(e.path()).ok()?).ok()
        })
        .collect();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    for s in &mut out {
        s.age_s = now.saturating_sub(s.started_ms) / 1000;
    }
    out.sort_by_key(|s| std::cmp::Reverse(s.started_ms));
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn only_safe_names_become_files() {
        assert!(super::file("872990894:1").is_some());
        assert!(super::file("../x").is_none());
        assert!(super::file("").is_none());
    }
}
