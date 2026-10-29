//! `device "<key>" { trackpad { } }` — how one phone's trackpad screen is laid out and what its
//! gestures do. Per device, because a phone held sideways on the couch and a tablet on a desk
//! want different things; the phone writes these through `ConfigSet`, and a device may change
//! its own without the settings grant (`wado_server::config::link`).
//!
//! Action ids (`quick`, `gestures`) are not checked here: the daemon's action library decides
//! what exists on this computer, and the phone skips an id it was not offered.

use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer, Serialize};

/// What the rail beside the pad holds.
pub const RAILS: &[&str] = &["keys", "actions", "none"];

/// The phone-only gestures an action can be given.
pub const GESTURES: &[&str] = &["edge-left", "edge-right", "four-tap", "two-hold"];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct DeviceTrackpad {
    /// The pad drawn landscape — the user's main way of holding it.
    pub landscape: bool,
    /// Fullscreen with a real orientation lock, rather than the pad drawn rotated.
    pub fullscreen: bool,
    /// Which way round the rotated layout faces.
    pub flip: bool,
    /// One of [`RAILS`].
    #[serde(deserialize_with = "rail")]
    pub rail: String,
    /// Action ids on the rail or quick bar, in order. A bare `quick` is none.
    #[serde(deserialize_with = "list")]
    pub quick: Vec<String>,
    /// A gesture from [`GESTURES`] → an action id. A gesture not named here is off.
    #[serde(deserialize_with = "gestures")]
    pub gestures: BTreeMap<String, String>,
    /// The keyboard is open when the pad opens.
    pub keyboard: bool,
}

impl Default for DeviceTrackpad {
    fn default() -> Self {
        Self {
            landscape: true,
            fullscreen: false,
            flip: false,
            rail: "keys".into(),
            quick: [
                "workspace-prev",
                "workspace-next",
                "fullscreen",
                "play-pause",
            ]
            .map(String::from)
            .to_vec(),
            gestures: BTreeMap::new(),
            keyboard: false,
        }
    }
}

/// What the phone is told (`ConfigState::trackpad`).
impl From<&DeviceTrackpad> for wado_protocol::TrackpadPrefs {
    fn from(t: &DeviceTrackpad) -> Self {
        Self {
            landscape: t.landscape,
            fullscreen: t.fullscreen,
            flip: t.flip,
            rail: t.rail.clone(),
            quick: t.quick.clone(),
            gestures: t.gestures.clone(),
            keyboard: t.keyboard,
        }
    }
}

fn rail<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    let s = String::deserialize(d)?;
    if RAILS.contains(&s.as_str()) {
        Ok(s)
    } else {
        Err(serde::de::Error::custom(format!(
            "rail is one of {}, not \"{s}\"",
            RAILS.join(", ")
        )))
    }
}

/// `quick "a" "b"`, `quick "a"`, or a bare `quick` (KDL reads that as `true`) for none.
fn list<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<String>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Ids {
        Bare(bool),
        One(String),
        Many(Vec<String>),
    }
    Ok(match Ids::deserialize(d)? {
        Ids::Bare(true) => Vec::new(),
        Ids::Bare(false) => {
            return Err(serde::de::Error::custom(
                "quick takes action ids, or nothing for none",
            ));
        }
        Ids::One(s) => vec![s],
        Ids::Many(v) => v,
    })
}

fn gestures<'de, D: Deserializer<'de>>(d: D) -> Result<BTreeMap<String, String>, D::Error> {
    let map = BTreeMap::<String, String>::deserialize(d)?;
    if let Some(k) = map.keys().find(|k| !GESTURES.contains(&k.as_str())) {
        return Err(serde::de::Error::custom(format!(
            "no phone gesture \"{k}\" — one of {}",
            GESTURES.join(", ")
        )));
    }
    Ok(map)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn written_under_its_device() {
        let c = crate::kdl::parse(
            "device \"k\" { trackpad { landscape #false; rail \"actions\"; quick \"fullscreen\" } }",
        )
        .unwrap();
        let t = c.device["k"].trackpad.clone().unwrap();
        assert!(!t.landscape);
        assert_eq!(t.rail, "actions");
        assert_eq!(t.quick, ["fullscreen"]);
        assert!(
            !t.fullscreen && !t.flip && !t.keyboard,
            "the rest are defaults"
        );
        assert!(t.gestures.is_empty());
    }

    #[test]
    fn defaults_are_the_brief() {
        let t = DeviceTrackpad::default();
        assert!(t.landscape && !t.fullscreen && !t.flip && !t.keyboard);
        assert_eq!(t.rail, "keys");
        assert_eq!(
            t.quick,
            [
                "workspace-prev",
                "workspace-next",
                "fullscreen",
                "play-pause"
            ]
        );
        assert!(t.gestures.is_empty(), "every gesture is off");
    }

    #[test]
    fn the_phone_is_told_every_field() {
        let mut t = DeviceTrackpad::default();
        t.gestures.insert("four-tap".into(), "lock".into());
        let p = wado_protocol::TrackpadPrefs::from(&t);
        assert_eq!(
            serde_json::to_value(&p).unwrap(),
            serde_json::json!({
                "landscape": true, "fullscreen": false, "flip": false, "rail": "keys",
                "quick": ["workspace-prev", "workspace-next", "fullscreen", "play-pause"],
                "gestures": {"four-tap": "lock"}, "keyboard": false
            })
        );
    }

    #[test]
    fn lists_gestures_and_an_empty_quick_bar() {
        let c = crate::kdl::parse(
            "device \"k\" { trackpad { quick \"a\" \"b\"; gestures { edge-left \"lock\" } } }\n\
             device \"j\" { trackpad { quick } }",
        )
        .unwrap();
        let k = c.device["k"].trackpad.clone().unwrap();
        assert_eq!(k.quick, ["a", "b"]);
        assert_eq!(k.gestures["edge-left"], "lock");
        assert!(c.device["j"].trackpad.clone().unwrap().quick.is_empty());
    }

    #[test]
    fn an_unknown_rail_or_gesture_is_an_error() {
        let e = crate::kdl::parse("device \"k\" {\n  trackpad {\n    rail \"left\"\n  }\n}")
            .unwrap_err();
        assert!(e.message.contains("rail"), "{e}");
        assert_eq!(e.line, 3, "{e}");
        assert!(
            crate::kdl::parse("device \"k\" { trackpad { gestures { five-tap \"lock\" } } }")
                .is_err()
        );
    }
}
