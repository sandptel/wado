//! One read of everything the panel shows, taken every second and after every action.
//!
//! All of it comes from outside the daemons — their sockets, the relay's `/health`, the files
//! the pool shares and the rig's logs — so the panel works the same with one daemon or ten, and
//! keeps working while they restart.

use std::sync::Arc;

use wado_config::Config;
use wado_protocol::SessionSummary;

use crate::cli::status::{self, Status};
use crate::gate::{Device, Gate, Grants, Request};

pub struct Row {
    pub device: Device,
    /// What it may do now — its line, or everything but files for the owner.
    pub grants: Grants,
    pub owner: bool,
    /// Watching a session right now.
    pub online: bool,
}

pub struct Snapshot {
    pub status: Status,
    pub devices: Vec<Row>,
    pub pending: Vec<Request>,
    pub cfg: Arc<Config>,
    /// Gate, files, shells and config lines from every daemon's log, oldest first.
    pub activity: Vec<String>,
}

impl Snapshot {
    pub fn read() -> Self {
        // Re-read the config each time: a change made here, in config.kdl or from a phone
        // shows up without restarting the panel.
        if let Ok(l) = wado_config::load_or_init() {
            wado_config::live::install(l.config);
        }
        let gate = Gate::default();
        let status = status::read();
        let watching: Vec<&str> = status
            .sessions
            .iter()
            .filter_map(|s| s.viewer.as_deref())
            .collect();
        let devices = gate
            .devices()
            .into_iter()
            .map(|d| Row {
                grants: gate.grants(&d.key),
                owner: crate::config::link::is_owner(&d.key, &gate),
                online: watching.contains(&d.name.as_str()),
                device: d,
            })
            .collect();
        Self {
            pending: gate.pending(),
            devices,
            cfg: wado_config::live::current(),
            activity: activity(),
            status,
        }
    }

    /// The session running on daemon `instance`, if any.
    pub fn session_on(&self, instance: &str) -> Option<&SessionSummary> {
        self.status
            .sessions
            .iter()
            .find(|s| s.instance.rsplit(':').next() == Some(instance))
    }
}

/// What a person running the computer wants to see happen: who came in, what they did.
const ACTIVITY: &[&str] = &[
    "gate:",
    "files:",
    "shells:",
    "config changed",
    "host action",
];

/// The newest activity lines across the rig's daemon logs, timestamps trimmed to the time.
///
/// ponytail: reads whole logs each second. They are rotated per restart and stay small; tail
/// from an offset if one ever grows past a few MB.
fn activity() -> Vec<String> {
    let Ok(rd) = std::fs::read_dir(status::rig_dir()) else {
        return Vec::new();
    };
    let mut lines: Vec<(String, String)> = rd
        .flatten()
        .filter(|e| {
            let n = e.file_name().to_string_lossy().to_string();
            n.starts_with("daemon-") && n.ends_with(".log")
        })
        .flat_map(|e| {
            let n = e.file_name().to_string_lossy().replace(".log", "");
            let tag = n.trim_start_matches("daemon-").to_string();
            std::fs::read_to_string(e.path())
                .unwrap_or_default()
                .lines()
                .map(strip_ansi)
                .filter(|l| ACTIVITY.iter().any(|a| l.contains(a)))
                .map(|l| (l.clone(), tag.clone()))
                .collect::<Vec<_>>()
        })
        .collect();
    lines.sort();
    lines
        .into_iter()
        .rev()
        .take(200)
        .rev()
        .map(|(l, tag)| tidy(&l, &tag))
        .collect()
}

/// `2026-10-05T16:44:32.17Z  INFO wado::gate: let X in` → `16:44:32 ²  let X in`.
fn tidy(line: &str, daemon: &str) -> String {
    let time = line.get(11..19).unwrap_or("");
    let msg = line
        .split_once(": ")
        .map_or(line, |(_, m)| m)
        .trim()
        .to_string();
    format!("{time} #{daemon} {msg}")
}

fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            for c in chars.by_ref() {
                if c.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_log_line_reads_as_activity() {
        let l = super::strip_ansi(
            "\x1b[2m2026-10-05T16:44:32.175Z\x1b[0m \x1b[32m INFO\x1b[0m wado::gate: gate: let Pixel in",
        );
        assert_eq!(super::tidy(&l, "1"), "16:44:32 #1 gate: let Pixel in");
    }
}
