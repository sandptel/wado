//! `device "<key>" { trackpad { } }` — how one phone's trackpad screen is laid out and what its
//! gestures do. Per device, because a phone held sideways on the couch and a tablet on a desk
//! want different things; the phone writes these through `ConfigSet`, and a device may change
//! its own without the settings grant (`wado_server::config::link`).
//!
//! The feel — pointer speed, scroll speed, natural scrolling, tapping, dragging — is libinput's
//! and the compositor's, not the phone's: the daemon applies these to *this* touchpad device on
//! the computer (Hyprland: `wado_server::platform::linux::hypr_device`), and the pad's declared
//! size stays the truth so libinput's millimetre thresholds stay true
//! (`docs/superpowers/research/2026-10-30-touchpad-feel.md` §3–§5). `click-bar`, `palm`, `marks`
//! and `gesture-distance` are the phone's own.
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
    /// Pointer speed, [`SPEED_MIN`]–[`SPEED_MAX`]× — libinput's speed for this device, set by
    /// the compositor (never by scaling the pad's declared size).
    #[serde(deserialize_with = "speed")]
    pub speed: f64,
    /// Two-finger scroll speed, [`SCROLL_MIN`]–[`SCROLL_MAX`]× (the compositor's scroll factor).
    #[serde(deserialize_with = "scroll_speed")]
    pub scroll_speed: f64,
    /// Natural scrolling; absent leaves the computer's own touchpad setting.
    pub natural: Option<bool>,
    /// Tap to click.
    pub tap: bool,
    /// Tap, then drag.
    pub tap_drag: bool,
    /// One of [`DRAG_LOCKS`]: whether a drag survives lifting the finger, and for how long.
    #[serde(deserialize_with = "drag_lock")]
    pub drag_lock: String,
    /// The phone draws a press bar to click with, for a computer with tapping off.
    pub click_bar: bool,
    /// The phone marks resting thumbs and large contacts as palms.
    pub palm: bool,
    /// The phone draws a mark under each finger.
    pub marks: bool,
    /// One of [`GESTURE_DISTANCES`]: how far or long wado's own phone gestures must go.
    #[serde(deserialize_with = "gesture_distance")]
    pub gesture_distance: String,
}

/// Pointer speed's range.
pub const SPEED_MIN: f64 = 0.5;
pub const SPEED_MAX: f64 = 2.0;
/// Scroll speed's range.
pub const SCROLL_MIN: f64 = 0.25;
pub const SCROLL_MAX: f64 = 2.0;
/// Drag lock: off, a short timeout after lifting, or until the next tap (libinput's 0 / 1 / 2).
pub const DRAG_LOCKS: &[&str] = &["off", "briefly", "until-tap"];
/// wado's own phone gestures: edge-in 28 / 40 / 56 px, two-finger hold 450 / 600 / 800 ms.
pub const GESTURE_DISTANCES: &[&str] = &["short", "medium", "long"];

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
            speed: 1.0,
            scroll_speed: 1.0,
            natural: None,
            tap: true,
            tap_drag: true,
            drag_lock: "off".into(),
            click_bar: false,
            palm: true,
            marks: true,
            gesture_distance: "medium".into(),
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
            speed: t.speed,
            scroll_speed: t.scroll_speed,
            natural: t.natural,
            tap: t.tap,
            tap_drag: t.tap_drag,
            drag_lock: t.drag_lock.clone(),
            click_bar: t.click_bar,
            palm: t.palm,
            marks: t.marks,
            gesture_distance: t.gesture_distance.clone(),
        }
    }
}

fn ranged<'de, D: Deserializer<'de>>(d: D, what: &str, lo: f64, hi: f64) -> Result<f64, D::Error> {
    let v = f64::deserialize(d)?;
    if (lo..=hi).contains(&v) {
        Ok(v)
    } else {
        Err(serde::de::Error::custom(format!(
            "{what} is from {lo} to {hi}, not {v}"
        )))
    }
}

fn speed<'de, D: Deserializer<'de>>(d: D) -> Result<f64, D::Error> {
    ranged(d, "speed", SPEED_MIN, SPEED_MAX)
}

fn scroll_speed<'de, D: Deserializer<'de>>(d: D) -> Result<f64, D::Error> {
    ranged(d, "scroll-speed", SCROLL_MIN, SCROLL_MAX)
}

fn one_of<'de, D: Deserializer<'de>>(d: D, what: &str, list: &[&str]) -> Result<String, D::Error> {
    let s = String::deserialize(d)?;
    if list.contains(&s.as_str()) {
        Ok(s)
    } else {
        Err(serde::de::Error::custom(format!(
            "{what} is one of {}, not \"{s}\"",
            list.join(", ")
        )))
    }
}

fn drag_lock<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    one_of(d, "drag-lock", DRAG_LOCKS)
}

fn gesture_distance<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    one_of(d, "gesture-distance", GESTURE_DISTANCES)
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
                "gestures": {"four-tap": "lock"}, "keyboard": false, "speed": 1.0,
                "scroll_speed": 1.0, "natural": null, "tap": true, "tap_drag": true,
                "drag_lock": "off", "click_bar": false, "palm": true, "marks": true,
                "gesture_distance": "medium"
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
    fn speed_is_read_and_checked() {
        let c = crate::kdl::parse(
            "device \"k\" { trackpad { speed 1.25 } }\ndevice \"j\" { trackpad { speed 2 } }",
        )
        .unwrap();
        assert_eq!(c.device["k"].trackpad.clone().unwrap().speed, 1.25);
        assert_eq!(c.device["j"].trackpad.clone().unwrap().speed, 2.0);
        assert_eq!(DeviceTrackpad::default().speed, 1.0);
        let e =
            crate::kdl::parse("device \"k\" {\n  trackpad {\n    speed 3.5\n  }\n}").unwrap_err();
        assert!(e.message.contains("speed"), "{e}");
        assert_eq!(e.line, 3, "{e}");
        assert!(crate::kdl::parse("device \"k\" { trackpad { speed 0.2 } }").is_err());
    }

    #[test]
    fn the_feel_is_read_with_its_defaults() {
        let d = DeviceTrackpad::default();
        assert_eq!(
            (d.scroll_speed, d.natural, d.tap, d.tap_drag),
            (1.0, None, true, true)
        );
        assert_eq!(
            (d.drag_lock.as_str(), d.gesture_distance.as_str()),
            ("off", "medium")
        );
        assert!(!d.click_bar && d.palm && d.marks);
        let c = crate::kdl::parse(
            r#"device "k" { trackpad { scroll-speed 0.5; natural #true; tap #false; tap-drag #false
               drag-lock "until-tap"; click-bar #true; palm #false; marks #false; gesture-distance "long" } }"#,
        )
        .unwrap();
        let t = c.device["k"].trackpad.clone().unwrap();
        assert_eq!(
            (t.scroll_speed, t.natural, t.tap, t.tap_drag),
            (0.5, Some(true), false, false)
        );
        assert_eq!(
            (t.drag_lock.as_str(), t.gesture_distance.as_str()),
            ("until-tap", "long")
        );
        assert!(t.click_bar && !t.palm && !t.marks);
    }

    #[test]
    fn a_feel_value_out_of_range_is_an_error_with_its_line() {
        let e = crate::kdl::parse("device \"k\" {\n  trackpad {\n    scroll-speed 3\n  }\n}")
            .unwrap_err();
        assert!(e.message.contains("scroll-speed"), "{e}");
        assert_eq!(e.line, 3, "{e}");
        for bad in [
            r#"device "k" { trackpad { scroll-speed 0.1 } }"#,
            r#"device "k" { trackpad { drag-lock "forever" } }"#,
            r#"device "k" { trackpad { gesture-distance "far" } }"#,
            r#"device "k" { trackpad { natural "yes" } }"#,
        ] {
            assert!(crate::kdl::parse(bad).is_err(), "{bad}");
        }
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
