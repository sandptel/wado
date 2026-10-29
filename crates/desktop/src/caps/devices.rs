// SPDX-License-Identifier: AGPL-3.0-only
//! The trust list (`~/.config/wado/trusted_clients`) through `wado::gate::Gate` — the same
//! reads and writes as `wado devices`, `wado allow`, `wado deny` and `wado unpair`.

pub use wado::gate::Grants;
use wado::gate::{Device, Gate};
use wado_protocol::{SessionSummary, link::DeviceLink};

#[derive(Clone)]
pub struct Row {
    pub device: Device,
    /// What it may do now: its line, or everything but files for the owner.
    pub grants: Grants,
    pub owner: bool,
    /// Watching a session right now.
    pub online: bool,
    /// How it reaches this computer, while it is joined to one of its daemons.
    pub link: Option<DeviceLink>,
}

pub fn list(sessions: &[SessionSummary]) -> Vec<Row> {
    let gate = Gate::default();
    let watching: Vec<&str> = sessions
        .iter()
        .filter_map(|s| s.viewer.as_deref())
        .collect();
    let links = wado::links::list();
    gate.devices()
        .into_iter()
        .map(|d| Row {
            link: links
                .iter()
                .find(|l| l.key == d.key)
                .map(|l| l.link.clone()),
            grants: gate.grants(&d.key),
            owner: wado::config::link::is_owner(&d.key, &gate),
            online: watching.contains(&d.name.as_str()),
            device: d,
        })
        .collect()
}

/// `wado allow|deny <device> <token>`. The line said back is what the CLI says.
pub fn set_grant(key: &str, token: &str, on: bool) -> Result<String, String> {
    Gate::default()
        .allow(key, &[token], on)
        .map(|(name, g)| format!("{name} may now: {}", wado::cli::devices::describe(&g)))
}

/// Files are one grant at three levels: 0 none · 1 read-only · 2 read-write.
pub fn set_files(key: &str, level: i32) -> Result<String, String> {
    match level {
        1 => set_grant(key, "files-ro", true),
        2 => set_grant(key, "files-rw", true),
        _ => set_grant(key, "files", false),
    }
}

pub fn unpair(key: &str) -> Result<String, String> {
    Gate::default()
        .unpair(key)
        .map(|name| format!("Unpaired {name}"))
}

/// A link in plain words, with its tone and how its dot moves (`link-words` in the UI).
pub fn link_words(l: Option<&DeviceLink>, watching: bool) -> (String, &'static str, &'static str) {
    match l {
        Some(l) => match l.state.as_str() {
            "direct" => {
                let ms = l.rtt_ms.map(|m| format!(" · {m} ms")).unwrap_or_default();
                let path = match l.path.as_str() {
                    "lan" => "Direct, same network",
                    "turn" => "Direct through TURN",
                    _ => "Direct",
                };
                (format!("{path}{ms}"), "good", "ripple")
            }
            "relayed" => ("Relayed · limited".into(), "caution", "breathe"),
            "reconnecting" => ("Reconnecting…".into(), "caution", "breathe"),
            _ => ("Offline".into(), "dim", "still"),
        },
        None if watching => ("Watching".into(), "good", "still"),
        None => ("Not connected".into(), "dim", "still"),
    }
}
