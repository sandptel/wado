// SPDX-License-Identifier: AGPL-3.0-only
//! The pool's running sessions (`wado::sessions`, `~/.config/wado/sessions/*.json`), and the link
//! of the device on each (`wado::links`).

pub use wado_protocol::SessionSummary;
use wado_protocol::link::DeviceLink;

/// The link reported by whoever is watching `s`, matched by device name.
pub fn viewer_link(s: &SessionSummary) -> Option<DeviceLink> {
    let viewer = s.viewer.as_deref()?;
    wado::links::list()
        .into_iter()
        .find(|l| l.name == viewer)
        .map(|l| l.link)
}

/// `872990894:2` → `2`.
pub fn daemon_of(s: &SessionSummary) -> String {
    s.instance
        .rsplit(':')
        .next()
        .unwrap_or(&s.instance)
        .to_string()
}

/// `3725` → `1 h 2 min`.
pub fn age(secs: u64) -> String {
    match secs {
        s if s < 60 => format!("{s} s"),
        s if s < 3600 => format!("{} min", s / 60),
        s if s < 86_400 => format!("{} h {} min", s / 3600, (s % 3600) / 60),
        s => format!("{} d {} h", s / 86_400, (s % 86_400) / 3600),
    }
}
