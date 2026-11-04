//! Continue, on the web: what was done on each computer, newest first — the shared rules
//! (`wado_ux::recent`, the app's `play.dart` too), kept in this browser's settings.

use dioxus::prelude::*;
pub use wado_ux::recent::{ago, gamed_lately, Recent};

use crate::state::Ui;

pub fn list(ui: Ui) -> Vec<Recent> {
    serde_json::from_str(&(ui.set.recent_play)()).unwrap_or_default()
}

/// Remember `kind` (`game`, `desktop`, `shell`, `file`) done on the selected computer.
pub fn note(ui: Ui, kind: &str, name: &str, exec: &str) {
    let r = Recent {
        computer: crate::game_mode::computer(ui),
        kind: kind.into(),
        name: name.into(),
        exec: exec.into(),
        at: crate::profile::now_s(),
    };
    let next = wado_ux::recent::note(&list(ui), r);
    let mut s = ui.set.recent_play;
    s.set(serde_json::to_string(&next).unwrap_or_default());
}
