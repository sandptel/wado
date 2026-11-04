//! Game mode: a switch on one app of one computer, set up once — how it is played, then the
//! session's rules — and used by every open of that app from this device (spec
//! `2026-11-04-player-home-and-devices`). While it runs, [`Profile::overlay`]'s values shadow the
//! device's own settings; neither client ever writes them over the person's.
//!
//! The overlay speaks the clients' shared setting names (the app's `Settings` keys, which the web
//! mirrors): a client applies the keys it has and ignores the rest.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// One app's Game mode, as both clients store it (`<computer>|<exec>` → this, as JSON).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Profile {
    pub name: String,
    pub exec: String,
    /// `pad` (on-screen), `controller` (one held by this device), `touch`.
    pub play: String,
    /// The on-screen pad's layout id.
    pub layout: String,
    /// `smooth`, `balanced`, `sharp` — see [`picture`].
    pub picture: String,
    /// Never miss a press: inputs sent twice, sound kept tight.
    pub input: bool,
    /// No stray gestures: three-finger and edge swipes off while playing.
    pub calm: bool,
    /// Volume buttons go to the game and the screen stays awake.
    pub awake: bool,
    /// The X server (Steam, X11-only games). Restart-only: it shapes how the session starts.
    pub x: bool,
    /// What the person changed while playing, kept for next time.
    pub kept: BTreeMap<String, Value>,
}

impl Default for Profile {
    fn default() -> Self {
        Self {
            name: String::new(),
            exec: String::new(),
            play: "pad".into(),
            layout: "xbox".into(),
            picture: "smooth".into(),
            input: true,
            calm: true,
            awake: true,
            x: false,
            kept: BTreeMap::new(),
        }
    }
}

/// A Picture preset's exact values.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Picture {
    pub fps: u32,
    /// `reactivity`, `balanced`, `quality` — the session config's own words.
    pub quality: &'static str,
    pub fps_lock: bool,
    /// Keyframes only when the picture asks (else periodic).
    pub on_request: bool,
}

/// The preset `id` on a screen refreshing `refresh` times a second.
pub fn picture(id: &str, refresh: u32) -> Picture {
    match id {
        "balanced" => Picture {
            fps: 60,
            quality: "balanced",
            fps_lock: true,
            on_request: true,
        },
        "sharp" => Picture {
            fps: 30,
            quality: "quality",
            fps_lock: false,
            on_request: false,
        },
        _ => Picture {
            fps: refresh.clamp(60, 120),
            quality: "reactivity",
            fps_lock: true,
            on_request: true,
        },
    }
}

/// The preset's values in a line, set in mono where shown: nothing hidden behind a name.
pub fn picture_words(id: &str, refresh: u32) -> String {
    let p = picture(id, refresh);
    let q = if p.quality == "reactivity" {
        "latency"
    } else {
        p.quality
    };
    format!(
        "{} fps · {q} · {} · {}",
        p.fps,
        if p.fps_lock { "locked" } else { "adaptive" },
        if p.on_request {
            "keyframes on demand"
        } else {
            "keyframes every few s"
        }
    )
}

/// Launchers and Windows-game runners need the X server.
pub fn needs_x(exec: &str) -> bool {
    let e = exec.to_lowercase();
    ["steam", "lutris", "heroic", "wine", "bottles", "itch"]
        .iter()
        .any(|k| e.contains(k))
}

/// A first profile for an app: launchers get the X server.
pub fn new(name: &str, exec: &str) -> Profile {
    Profile {
        name: name.into(),
        exec: exec.into(),
        x: needs_x(exec),
        ..Profile::default()
    }
}

/// How it is played, in words.
pub fn play_words(play: &str) -> &'static str {
    match play {
        "controller" => "controller",
        "touch" => "touch",
        _ => "on-screen pad",
    }
}

impl Profile {
    /// The settings it shadows while it runs, by the clients' shared names.
    pub fn overlay(&self, refresh: u32) -> BTreeMap<String, Value> {
        let mut o = BTreeMap::new();
        let mut put = |k: &str, v: Value| {
            o.insert(k.to_string(), v);
        };
        put("cockpit", json!(true));
        put("console_open", json!("full"));
        match self.play.as_str() {
            "touch" => {
                put("console_face", json!("touchpad"));
                put("pad_on", json!(false));
            }
            "controller" => put("pad_on", json!(false)),
            _ => {
                put("console_face", json!("gamepad"));
                put("pad_on", json!(true));
                put("gpad_layout", json!(self.layout));
            }
        }
        let p = picture(&self.picture, refresh);
        put("fps", json!(p.fps));
        put("quality", json!(p.quality));
        put("fps_lock", json!(p.fps_lock));
        put(
            "keyframe_mode",
            json!(if p.on_request {
                "on_request"
            } else {
                "periodic"
            }),
        );
        if self.input {
            put("redundant_input", json!(true));
            put("low_latency_audio", json!(true));
        }
        if self.calm {
            put("edge_swipe", json!(false));
            put("swipe3", json!(false));
        }
        if self.awake {
            put("keep_awake", json!(true));
            put("volume_keys", json!(true));
        }
        if self.x {
            put("x_server", json!(true));
        }
        for (k, v) in &self.kept {
            o.insert(k.clone(), v.clone());
        }
        o
    }

    /// A shadowed setting changed in the game: keep it for that game.
    pub fn keep(&mut self, key: &str, value: Value) {
        match (key, value.as_str()) {
            ("gpad_layout", Some(l)) => self.layout = l.to_string(),
            _ => {
                self.kept.insert(key.to_string(), value);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pad_game_shadows_full_screen_the_pad_and_its_picture() {
        let p = Profile {
            layout: "racing".into(),
            picture: "sharp".into(),
            x: true,
            ..new("Celeste", "celeste")
        };
        let o = p.overlay(120);
        assert_eq!(o["cockpit"], json!(true));
        assert_eq!(o["console_face"], json!("gamepad"));
        assert_eq!(o["gpad_layout"], json!("racing"));
        assert_eq!(o["fps"], json!(30));
        assert_eq!(o["x_server"], json!(true));
        assert_eq!(o["swipe3"], json!(false));
        assert_eq!(picture("smooth", 144).fps, 120, "the screen's rate, capped");
    }

    #[test]
    fn what_changed_in_the_game_is_kept_and_wins() {
        let mut p = new("Hades", "hades");
        p.keep("cockpit", json!(false));
        p.keep("gpad_layout", json!("shapes"));
        let o = p.overlay(60);
        assert_eq!(o["cockpit"], json!(false));
        assert_eq!(p.layout, "shapes");
        assert!(new("Steam", "/usr/bin/steam %U").x && !new("Celeste", "celeste").x);
    }

    #[test]
    fn a_profile_survives_json_including_an_old_one_with_fields_missing() {
        let p = new("Hades", "hades");
        let q: Profile = serde_json::from_str(&serde_json::to_string(&p).unwrap()).unwrap();
        assert_eq!(p, q);
        let old: Profile =
            serde_json::from_str(r#"{"name":"A","exec":"a","picture":"balanced"}"#).unwrap();
        assert_eq!((old.play.as_str(), old.input), ("pad", true));
    }
}
