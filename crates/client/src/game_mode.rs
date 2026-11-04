//! Game mode, on the web (spec `2026-11-04-player-home-and-devices`, the app's `game_mode.dart`):
//! a switch on one app of one computer, set up once. Its rules shape the session config while a
//! session runs for it — the picture preset, sound kept tight, the X server — and never touch the
//! person's own settings.
//!
//! ponytail: the app also opens a game full screen with its pad; on the web the console's face and
//! arrangement are the computer's per-device prefs (`device { console }`), so a game would write
//! the computer's config. Here the rules are the session's only; the face stays the person's.

use std::collections::BTreeMap;

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};
use wado_protocol::{Quality, SessionConfig};

use crate::state::Ui;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Profile {
    pub name: String,
    pub exec: String,
    /// `smooth`, `balanced` or `sharp` — see [`picture`].
    pub picture: String,
    /// Sound kept tight.
    pub input: bool,
    /// The X server: Steam and X11-only games.
    pub x: bool,
}

impl Default for Profile {
    fn default() -> Self {
        Self {
            name: String::new(),
            exec: String::new(),
            picture: "smooth".into(),
            input: true,
            x: false,
        }
    }
}

/// Launchers and Windows-game runners need the X server.
pub fn needs_x(exec: &str) -> bool {
    let e = exec.to_lowercase();
    ["steam", "lutris", "heroic", "wine", "bottles", "itch"]
        .iter()
        .any(|k| e.contains(k))
}

/// (fps, quality, keyframes on request) for a preset — the same three as the app.
pub fn picture(id: &str) -> (u32, Quality, bool) {
    match id {
        "balanced" => (60, Quality::Balanced, true),
        "sharp" => (30, Quality::Quality, false),
        _ => (120, Quality::Reactivity, true),
    }
}

pub fn words(id: &str) -> String {
    let (fps, q, kf) = picture(id);
    let q = match q {
        Quality::Reactivity => "latency",
        Quality::Quality => "quality",
        _ => "balanced",
    };
    format!(
        "{fps} fps · {q} · {}",
        if kf {
            "keyframes on demand"
        } else {
            "keyframes every few s"
        }
    )
}

/// The selected computer, as its live status is filed (`profile::Profile::key`).
pub fn computer(ui: Ui) -> String {
    format!("{}|{}", (ui.set.relay_url)(), (ui.set.remote_id)())
}

fn all(ui: Ui) -> BTreeMap<String, Profile> {
    serde_json::from_str(&(ui.set.game_modes)()).unwrap_or_default()
}

/// Every app of the selected computer with Game mode on.
pub fn on(ui: Ui) -> Vec<Profile> {
    let c = format!("{}#", computer(ui));
    all(ui)
        .into_iter()
        .filter(|(k, _)| k.starts_with(&c))
        .map(|(_, p)| p)
        .collect()
}

pub fn save(ui: Ui, p: &Profile) {
    let mut m = all(ui);
    m.insert(format!("{}#{}", computer(ui), p.exec), p.clone());
    let mut s = ui.set.game_modes;
    s.set(serde_json::to_string(&m).unwrap_or_default());
}

pub fn off(ui: Ui, exec: &str) {
    let mut m = all(ui);
    m.remove(&format!("{}#{exec}", computer(ui)));
    let mut s = ui.set.game_modes;
    s.set(serde_json::to_string(&m).unwrap_or_default());
}

/// The running game's rules over the session config.
pub fn shape(c: &mut SessionConfig, p: &Profile) {
    let (fps, q, on_request) = picture(&p.picture);
    c.fps = fps;
    c.quality = q;
    c.keyframe_interval = on_request.then_some(fps * 10);
    if p.input {
        c.low_latency_audio = true;
    }
    if p.x {
        c.x_server = true;
    }
}

/// Open `p`'s app as a game: a session shaped by its rules, the app launched once it is up
/// (`games::launcher`), or straight into the running session.
pub fn open(ui: Ui, p: Profile) {
    let mut live = ui.live;
    live.game.set(Some(p.clone()));
    live.game_launch.set(Some(p.exec.clone()));
    if (live.session_on)() {
        crate::actions::launch_command(ui, &p.exec);
        live.game_launch.set(None);
    } else {
        crate::actions::start(ui);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn presets_and_launchers_read_as_the_app_does() {
        assert_eq!(super::picture("sharp").0, 30);
        assert!(!super::picture("sharp").2, "sharp keeps periodic keyframes");
        assert_eq!(
            super::picture("anything").0,
            120,
            "smoothest is the default"
        );
        assert!(super::needs_x("/usr/bin/steam %U") && !super::needs_x("celeste"));
    }
}
