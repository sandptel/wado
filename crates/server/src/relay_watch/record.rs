//! Each daemon's link to its relay, as files: `~/.config/wado/relay/<instance>.json` — what
//! `wado relay status`, `wado` and `wado tui` read. Same shape as [`crate::sessions`]: one file
//! per daemon, rewritten on every change and every few seconds, so its age is the heartbeat.

use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// A file not rewritten for this long belongs to a daemon that is not running.
pub const STALE: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Record {
    /// This daemon's instance number.
    pub instance: String,
    /// The relay it dials.
    pub url: String,
    /// Registered with the relay right now.
    pub up: bool,
    /// Unix ms the link last came up (when `up`) or was lost / first failed (when not).
    pub since_ms: u64,
    /// Why the last dial or link failed, in the daemon's words.
    pub error: String,
    /// The relay's caps, from its last `registered`.
    pub caps: Vec<String>,
    /// Links lost since the daemon started.
    pub outages: u32,
    /// The longest of them, ms.
    pub longest_ms: u64,
    /// The relay was down long enough to tell the devices (`super::push`).
    pub told: bool,
    /// Unix ms this was written.
    pub at_ms: u64,
}

impl Record {
    /// How long the current state has held, as of `now_ms`.
    pub fn for_ms(&self, now_ms: u64) -> u64 {
        now_ms.saturating_sub(self.since_ms)
    }
}

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn dir() -> PathBuf {
    wado_config::paths::config_dir().join("relay")
}

fn file(instance: &str) -> Option<PathBuf> {
    let safe = !instance.is_empty()
        && instance
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c));
    safe.then(|| dir().join(format!("{instance}.json")))
}

pub fn write(r: &Record) {
    let Some(path) = file(&r.instance) else {
        return;
    };
    let _ = std::fs::create_dir_all(dir());
    let tmp = path.with_extension("tmp");
    if let Ok(json) = serde_json::to_vec(r)
        && std::fs::write(&tmp, json).is_ok()
    {
        let _ = std::fs::rename(&tmp, &path);
    }
}

/// Every running daemon's link, by instance. Stale files are left alone (a daemon may be
/// restarting) and simply not listed.
pub fn list() -> Vec<Record> {
    let Ok(rd) = std::fs::read_dir(dir()) else {
        return Vec::new();
    };
    let now = now_ms();
    let mut out: Vec<Record> = rd
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
        .filter_map(|e| std::fs::read(e.path()).ok())
        .filter_map(|b| serde_json::from_slice::<Record>(&b).ok())
        .filter(|r| now.saturating_sub(r.at_ms) < STALE.as_millis() as u64)
        .collect();
    out.sort_by_key(|r| (r.instance.len(), r.instance.clone()));
    out
}

/// `3 h 12 min`, `4 min`, `12 s` — how a person says a span.
pub fn span(ms: u64) -> String {
    let s = ms / 1000;
    match s {
        0..60 => format!("{s} s"),
        60..3600 => format!("{} min", s / 60),
        3600..86400 => format!("{} h {} min", s / 3600, s % 3600 / 60),
        _ => format!("{} d {} h", s / 86400, s % 86400 / 3600),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spans_read_like_a_person_says_them() {
        assert_eq!(span(12_000), "12 s");
        assert_eq!(span(4 * 60_000 + 5_000), "4 min");
        assert_eq!(span(3 * 3_600_000 + 12 * 60_000), "3 h 12 min");
        assert_eq!(span(2 * 86_400_000 + 3_600_000), "2 d 1 h");
    }

    #[test]
    fn an_older_file_still_reads() {
        let r: Record = serde_json::from_str(r#"{"instance":"1","up":true}"#).unwrap();
        assert!(r.up && r.caps.is_empty());
    }
}
