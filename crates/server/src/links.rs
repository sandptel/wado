//! How each connected device reaches this computer, as files: `~/.config/wado/links/<instance>.json`.
//!
//! The device reports its link every few seconds (`RelayMsg::DeviceStatus`). The daemon it is on
//! writes the latest here, so `wado tui` and `wado` can show every device of the pool, and a
//! change of path is logged as it happens. One file per daemon, because a daemon has one seat.
//! Same shape as [`crate::sessions`]: the file's age is the heartbeat.

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::Mutex,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use wado_protocol::link::DeviceLink;

/// Three missed reports: the device is gone, or its daemon is.
const STALE: Duration = Duration::from_secs(20);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    /// The device's `client_key` — what the trust list knows it by.
    pub key: String,
    pub name: String,
    pub link: DeviceLink,
    /// When the device last reported, unix ms.
    pub at_ms: u64,
}

fn dir() -> PathBuf {
    wado_config::paths::config_dir().join("links")
}

fn file(remote_id: &str) -> PathBuf {
    let instance: String = crate::instance::instance_key()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || "-_.".contains(*c))
        .collect();
    let id = wado_protocol::relay::normalize_remote_id(remote_id);
    dir().join(format!("{id}_{instance}.json"))
}

/// The last report per daemon, to log changes rather than every heartbeat.
fn last() -> &'static Mutex<HashMap<PathBuf, DeviceLink>> {
    static LAST: std::sync::OnceLock<Mutex<HashMap<PathBuf, DeviceLink>>> =
        std::sync::OnceLock::new();
    LAST.get_or_init(Default::default)
}

/// Record a device's report. Returns a line worth logging when its path changed.
pub fn report(remote_id: &str, key: &str, name: &str, link: DeviceLink) -> Option<String> {
    let path = file(remote_id);
    let at_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64);
    let entry = Entry {
        key: key.into(),
        name: name.into(),
        link,
        at_ms,
    };
    let _ = std::fs::create_dir_all(dir());
    let tmp = path.with_extension("tmp");
    if let Ok(json) = serde_json::to_vec(&entry) {
        if std::fs::write(&tmp, json).is_ok() {
            let _ = std::fs::rename(&tmp, &path);
        }
    }
    let mut last = last().lock().unwrap_or_else(|e| e.into_inner());
    let before = last.insert(path, entry.link.clone());
    changed(before.as_ref(), &entry.link).then(|| format!("{name}: {}", describe(&entry.link)))
}

/// The device left this daemon.
pub fn clear(remote_id: &str) {
    let path = file(remote_id);
    let _ = std::fs::remove_file(&path);
    last()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(&path);
}

/// Every device reporting to any daemon of this computer. Stale files are removed on the way.
pub fn list() -> Vec<Entry> {
    let Ok(rd) = std::fs::read_dir(dir()) else {
        return Vec::new();
    };
    rd.flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
        .filter_map(|e| {
            let fresh = e.metadata().ok()?.modified().ok()?.elapsed().ok()? < STALE;
            if !fresh {
                let _ = std::fs::remove_file(e.path());
                return None;
            }
            serde_json::from_slice(&std::fs::read(e.path()).ok()?).ok()
        })
        .collect()
}

/// A path change is news; a new RTT on the same path is not.
fn changed(before: Option<&DeviceLink>, now: &DeviceLink) -> bool {
    before.is_none_or(|b| b.state != now.state || b.path != now.path || b.net != now.net)
}

/// `direct · lan · 4 ms · wifi`, the way the panel and the log show a link.
pub fn describe(l: &DeviceLink) -> String {
    let mut parts = vec![if l.state.is_empty() { "?" } else { &l.state }.to_string()];
    if !l.path.is_empty() {
        parts.push(l.path.clone());
    }
    if let Some(ms) = l.rtt_ms {
        parts.push(format!("{ms} ms"));
    }
    if !l.net.is_empty() {
        parts.push(l.net.clone());
    }
    parts.join(" · ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn link(state: &str, path: &str, rtt: Option<u32>) -> DeviceLink {
        DeviceLink {
            state: state.into(),
            path: path.into(),
            rtt_ms: rtt,
            net: "wifi".into(),
            ..Default::default()
        }
    }

    #[test]
    fn only_a_new_path_is_news() {
        let a = link("direct", "lan", Some(4));
        assert!(changed(None, &a), "first report");
        assert!(
            !changed(Some(&a), &link("direct", "lan", Some(9))),
            "rtt alone"
        );
        assert!(changed(Some(&a), &link("relayed", "", None)));
        assert_eq!(describe(&a), "direct · lan · 4 ms · wifi");
    }
}
