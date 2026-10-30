//! `device "<key>" { console { } }` — how one device opens a session: in the Pocket Console (the
//! picture inset, the controls around it) or full screen, and which input face the deck shows.
//! Per device, because a phone and a desktop browser want different faces; the device writes
//! these itself through `ConfigSet`, without the settings grant, as it does its trackpad
//! (`wado_server::config::link`).

use serde::{Deserialize, Deserializer, Serialize};

/// How a session opens.
pub const OPENS: &[&str] = &["console", "full"];

/// The deck's faces. `auto` is the device's own default: the Touchpad on touch, Quick actions
/// with a mouse (the page decides, since only it knows the pointer).
pub const FACES: &[&str] = &["auto", "pad", "keys", "quick", "gamepad"];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct DeviceConsole {
    /// One of [`OPENS`].
    #[serde(deserialize_with = "open")]
    pub open: String,
    /// One of [`FACES`].
    #[serde(deserialize_with = "face")]
    pub face: String,
}

impl Default for DeviceConsole {
    fn default() -> Self {
        Self {
            open: "console".into(),
            face: "auto".into(),
        }
    }
}

/// What the device is told (`ConfigState::console`).
impl From<&DeviceConsole> for wado_protocol::ConsolePrefs {
    fn from(c: &DeviceConsole) -> Self {
        Self {
            open: c.open.clone(),
            face: c.face.clone(),
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

fn open<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    one_of(d, "open", OPENS)
}

fn face<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    one_of(d, "face", FACES)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn written_under_its_device() {
        let c =
            crate::kdl::parse("device \"k\" { console { open \"full\"; face \"keys\" } }").unwrap();
        let t = c.device["k"].console.clone().unwrap();
        assert_eq!(t.open, "full");
        assert_eq!(t.face, "keys");
    }

    #[test]
    fn defaults_open_the_console_on_the_devices_own_face() {
        let t = DeviceConsole::default();
        assert_eq!((t.open.as_str(), t.face.as_str()), ("console", "auto"));
        let c = crate::kdl::parse("device \"k\" { console { } }").unwrap();
        assert_eq!(c.device["k"].console.clone().unwrap(), t);
    }

    #[test]
    fn the_device_is_told_every_field() {
        let p = wado_protocol::ConsolePrefs::from(&DeviceConsole::default());
        assert_eq!(
            serde_json::to_value(&p).unwrap(),
            serde_json::json!({ "open": "console", "face": "auto" })
        );
    }

    #[test]
    fn an_unknown_value_is_an_error() {
        let e = crate::kdl::parse("device \"k\" {\n  console {\n    face \"mouse\"\n  }\n}")
            .unwrap_err();
        assert!(e.message.contains("face"), "{e}");
        assert_eq!(e.line, 3, "{e}");
        assert!(crate::kdl::parse("device \"k\" { console { open \"window\" } }").is_err());
    }
}
