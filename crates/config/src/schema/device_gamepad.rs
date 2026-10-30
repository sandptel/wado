//! `device "<key>" { gamepad { } }` — one device's on-screen gamepad: what its buttons drive, which
//! layout it shows in a session and away from one, its own saved layouts, and two choices shared
//! by all three virtual input devices (haptics, which hand). Per device; the device writes these
//! itself through `ConfigSet`, without the settings grant, as it does its trackpad
//! (`wado_server::config::link`).
//!
//! A saved layout is written by the gamepad's editor, never by hand, so its controls travel as
//! the client's own compact JSON (the daemon does not read inside it — the same reasoning as
//! `device { prefs }`): `mine { mine-3f2a "Couch racer" "racing" "{…}" }`, one child per
//! layout, named by its id, with its name, the built-in it was made from, and its changes.
//! Everything a person might edit by hand is typed.

use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer, Serialize};

/// What the buttons drive: a real controller, or keys and the mouse (in a session).
pub const DRIVES: &[&str] = &["pad", "keys"];
/// How strongly the phone ticks under a finger, for the touchpad and keyboard too.
pub const HAPTICS: &[&str] = &["off", "light", "medium", "strong"];
/// Which hand the controls are drawn for, for the touchpad's rail too.
pub const HANDS: &[&str] = &["right", "left"];
/// At most this many saved layouts per device.
pub const MINE_MAX: usize = 8;
/// A saved layout's JSON is at most this many bytes.
pub const JSON_MAX: usize = 4096;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct DeviceGamepad {
    /// One of [`DRIVES`].
    #[serde(deserialize_with = "drives")]
    pub drives: String,
    /// The layout in a session and on the console's deck: a built-in id or a saved one's.
    #[serde(deserialize_with = "layout")]
    pub layout: String,
    /// Away from a session: `same` (the session's layout), or a layout id.
    #[serde(deserialize_with = "layout")]
    pub away: String,
    /// One of [`HAPTICS`].
    #[serde(deserialize_with = "haptics")]
    pub haptics: String,
    /// One of [`HANDS`].
    #[serde(deserialize_with = "hand")]
    pub hand: String,
    /// Saved layouts by id (`mine-…`): `[name, base, json]`.
    #[serde(deserialize_with = "mine")]
    pub mine: BTreeMap<String, [String; 3]>,
}

impl Default for DeviceGamepad {
    fn default() -> Self {
        Self {
            drives: "pad".into(),
            layout: "xbox".into(),
            away: "same".into(),
            haptics: "medium".into(),
            hand: "right".into(),
            mine: BTreeMap::new(),
        }
    }
}

/// What the device is told (`ConfigState::gamepad`).
impl From<&DeviceGamepad> for wado_protocol::GamepadPrefs {
    fn from(g: &DeviceGamepad) -> Self {
        Self {
            drives: g.drives.clone(),
            layout: g.layout.clone(),
            away: g.away.clone(),
            haptics: g.haptics.clone(),
            hand: g.hand.clone(),
            mine: g
                .mine
                .iter()
                .map(|(id, [name, base, json])| wado_protocol::SavedLayout {
                    id: id.clone(),
                    name: name.clone(),
                    base: base.clone(),
                    json: json.clone(),
                })
                .collect(),
        }
    }
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

fn drives<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    one_of(d, "drives", DRIVES)
}

fn haptics<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    one_of(d, "haptics", HAPTICS)
}

fn hand<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    one_of(d, "hand", HANDS)
}

fn layout<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    let s = String::deserialize(d)?;
    if s.trim().is_empty() {
        return Err(serde::de::Error::custom("a layout is named by its id"));
    }
    Ok(s)
}

fn mine<'de, D: Deserializer<'de>>(d: D) -> Result<BTreeMap<String, [String; 3]>, D::Error> {
    use serde::de::Error;
    let raw = BTreeMap::<String, Vec<String>>::deserialize(d)?;
    if raw.len() > MINE_MAX {
        return Err(D::Error::custom(format!(
            "at most {MINE_MAX} saved layouts, not {}",
            raw.len()
        )));
    }
    raw.into_iter()
        .map(|(id, v)| {
            if !id.starts_with("mine-") {
                return Err(D::Error::custom(format!(
                    "a saved layout's id starts with \"mine-\", not \"{id}\""
                )));
            }
            let Ok([name, base, json]) = <[String; 3]>::try_from(v) else {
                return Err(D::Error::custom(format!(
                    "saved layout \"{id}\" is a name, the layout it was made from, and its JSON"
                )));
            };
            if json.len() > JSON_MAX {
                return Err(D::Error::custom(format!(
                    "saved layout \"{id}\" is {} bytes, more than {JSON_MAX}",
                    json.len()
                )));
            }
            if !matches!(
                serde_json::from_str::<serde_json::Value>(&json),
                Ok(serde_json::Value::Object(_))
            ) {
                return Err(D::Error::custom(format!(
                    "saved layout \"{id}\" is not a JSON object"
                )));
            }
            Ok((id, [name, base, json]))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn written_under_its_device() {
        let c = crate::kdl::parse(
            r#"device "k" { gamepad { drives "keys"; layout "mine-1"; away "racing"; haptics "strong"; hand "left"
                mine { mine-1 "Couch racer" "racing" "{\"v\":1}" } } }"#,
        )
        .unwrap();
        let g = c.device["k"].gamepad.clone().unwrap();
        assert_eq!(
            (g.drives.as_str(), g.layout.as_str(), g.away.as_str()),
            ("keys", "mine-1", "racing")
        );
        assert_eq!((g.haptics.as_str(), g.hand.as_str()), ("strong", "left"));
        assert_eq!(g.mine["mine-1"], ["Couch racer", "racing", r#"{"v":1}"#]);
    }

    #[test]
    fn defaults_are_a_real_controller_xbox_style_the_same_away() {
        let g = DeviceGamepad::default();
        assert_eq!(
            [&g.drives, &g.layout, &g.away, &g.haptics, &g.hand],
            ["pad", "xbox", "same", "medium", "right"]
        );
        assert!(g.mine.is_empty());
        let c = crate::kdl::parse("device \"k\" { gamepad { } }").unwrap();
        assert_eq!(c.device["k"].gamepad.clone().unwrap(), g);
    }

    #[test]
    fn the_device_is_told_every_field() {
        let mut g = DeviceGamepad::default();
        g.mine
            .insert("mine-a".into(), ["A".into(), "ps".into(), "{}".into()]);
        let p = wado_protocol::GamepadPrefs::from(&g);
        assert_eq!(
            serde_json::to_value(&p).unwrap(),
            serde_json::json!({"drives":"pad","layout":"xbox","away":"same","haptics":"medium",
                "hand":"right","mine":[{"id":"mine-a","name":"A","base":"ps","json":"{}"}]})
        );
    }

    #[test]
    fn unknown_values_are_errors_with_their_line() {
        let e = crate::kdl::parse("device \"k\" {\n  gamepad {\n    haptics \"buzz\"\n  }\n}")
            .unwrap_err();
        assert!(e.message.contains("haptics"), "{e}");
        assert_eq!(e.line, 3, "{e}");
        for bad in [
            r#"device "k" { gamepad { drives "mouse" } }"#,
            r#"device "k" { gamepad { hand "both" } }"#,
            r#"device "k" { gamepad { layout "" } }"#,
        ] {
            assert!(crate::kdl::parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn saved_layouts_are_checked() {
        for bad in [
            // not a mine- id
            r#"device "k" { gamepad { mine { xbox "X" "xbox" "{}" } } }"#,
            // two values, not three
            r#"device "k" { gamepad { mine { mine-1 "X" "xbox" } } }"#,
            // not an object
            r#"device "k" { gamepad { mine { mine-1 "X" "xbox" "[1]" } } }"#,
            r#"device "k" { gamepad { mine { mine-1 "X" "xbox" "{" } } }"#,
        ] {
            assert!(crate::kdl::parse(bad).is_err(), "{bad}");
        }
        let big = format!(
            "device \"k\" {{ gamepad {{ mine {{ mine-1 \"X\" \"xbox\" \"{{\\\"a\\\":\\\"{}\\\"}}\" }} }} }}",
            "x".repeat(JSON_MAX)
        );
        assert!(
            crate::kdl::parse(&big)
                .unwrap_err()
                .message
                .contains("bytes")
        );
        let nine: String = (0..9)
            .map(|i| format!("mine-{i} \"L{i}\" \"xbox\" \"{{}}\"\n"))
            .collect();
        let e = crate::kdl::parse(&format!(
            "device \"k\" {{ gamepad {{ mine {{\n{nine}}} }} }}"
        ))
        .unwrap_err();
        assert!(e.message.contains("at most 8"), "{e}");
    }
}
