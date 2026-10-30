//! `device "<key>" { keyboard { } }` — the keys one device puts above its phone keyboard, the
//! ones a phone keyboard does not have (Esc, Tab, the arrows…). Per device and in order; the
//! device writes them itself through `ConfigSet`, without the settings grant, as it does its
//! trackpad (`wado_server::config::link`).

use serde::{Deserialize, Deserializer, Serialize};

/// The keys that can go above the keyboard — names the pad's key messages carry
/// (`wado_server::pad::keys::key_code`).
pub const KEYS: &[&str] = &[
    "esc", "tab", "left", "up", "down", "right", "home", "end", "pageup", "pagedown", "delete",
    "enter", "f1", "f2", "f3", "f4", "f5", "f6", "f7", "f8", "f9", "f10", "f11", "f12",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct DeviceKeyboard {
    /// Keys from [`KEYS`], in the order they are shown. A bare `keys` is none.
    #[serde(deserialize_with = "keys")]
    pub keys: Vec<String>,
}

impl Default for DeviceKeyboard {
    fn default() -> Self {
        Self {
            keys: ["esc", "tab", "left", "up", "down", "right"]
                .map(String::from)
                .to_vec(),
        }
    }
}

/// What the device is told (`ConfigState::keyboard`).
impl From<&DeviceKeyboard> for wado_protocol::KeyboardPrefs {
    fn from(k: &DeviceKeyboard) -> Self {
        Self {
            keys: k.keys.clone(),
        }
    }
}

/// `keys "a" "b"`, `keys "a"`, or a bare `keys` (KDL reads that as `true`) for none.
fn keys<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<String>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Names {
        Bare(bool),
        One(String),
        Many(Vec<String>),
    }
    let list = match Names::deserialize(d)? {
        Names::Bare(true) => Vec::new(),
        Names::Bare(false) => {
            return Err(serde::de::Error::custom(
                "keys takes key names, or nothing for none",
            ));
        }
        Names::One(s) => vec![s],
        Names::Many(v) => v,
    };
    if let Some(k) = list.iter().find(|k| !KEYS.contains(&k.as_str())) {
        return Err(serde::de::Error::custom(format!(
            "no key \"{k}\" above the keyboard — one of {}",
            KEYS.join(", ")
        )));
    }
    Ok(list)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn written_under_its_device_in_order() {
        let c = crate::kdl::parse("device \"k\" { keyboard { keys \"f5\" \"esc\" \"pageup\" } }")
            .unwrap();
        assert_eq!(
            c.device["k"].keyboard.clone().unwrap().keys,
            ["f5", "esc", "pageup"]
        );
    }

    #[test]
    fn defaults_are_esc_tab_and_the_arrows() {
        assert_eq!(
            DeviceKeyboard::default().keys,
            ["esc", "tab", "left", "up", "down", "right"]
        );
        let c = crate::kdl::parse("device \"k\" { keyboard { } }").unwrap();
        assert_eq!(
            c.device["k"].keyboard.clone().unwrap(),
            DeviceKeyboard::default()
        );
    }

    #[test]
    fn a_bare_keys_is_none_and_one_is_one() {
        let c = crate::kdl::parse(
            "device \"k\" { keyboard { keys } }\ndevice \"j\" { keyboard { keys \"tab\" } }",
        )
        .unwrap();
        assert!(c.device["k"].keyboard.clone().unwrap().keys.is_empty());
        assert_eq!(c.device["j"].keyboard.clone().unwrap().keys, ["tab"]);
    }

    #[test]
    fn the_device_is_told_its_keys() {
        let p = wado_protocol::KeyboardPrefs::from(&DeviceKeyboard::default());
        assert_eq!(
            serde_json::to_value(&p).unwrap(),
            serde_json::json!({ "keys": ["esc", "tab", "left", "up", "down", "right"] })
        );
    }

    #[test]
    fn an_unknown_key_is_an_error_with_its_line() {
        let e =
            crate::kdl::parse("device \"k\" {\n  keyboard {\n    keys \"esc\" \"meta\"\n  }\n}")
                .unwrap_err();
        assert!(e.message.contains("meta"), "{e}");
        assert_eq!(e.line, 3, "{e}");
    }
}
