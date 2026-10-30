//! `device "<key>" { size { } }` — how big one device draws the client: its text size and its
//! interface size (DESIGN.md, "Size"). Per device, because a phone held at arm's length and a
//! desktop monitor want different sizes; the device writes these itself through `ConfigSet`,
//! without the settings grant, as it does its trackpad and console (`wado_server::config::link`).

use serde::{Deserialize, Deserializer, Serialize};

/// The interface sizes, smallest first. `cozy` is the design's own size.
pub const INTERFACES: &[&str] = &["compact", "cozy", "comfortable", "large"];

/// Text size, in percent of the design's own: from `TEXT_MIN` to `TEXT_MAX` in `TEXT_STEP`s.
pub const TEXT_MIN: u32 = 85;
pub const TEXT_MAX: u32 = 150;
pub const TEXT_STEP: u32 = 5;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct DeviceSize {
    /// Text size in percent ([`TEXT_MIN`]–[`TEXT_MAX`], a multiple of [`TEXT_STEP`]).
    #[serde(deserialize_with = "text")]
    pub text: u32,
    /// Text follows the device's own setting instead: the phone's font size in the Android app,
    /// the browser's default size in a browser. `text` is kept for when this is turned off.
    pub follow: bool,
    /// One of [`INTERFACES`].
    #[serde(deserialize_with = "interface")]
    pub interface: String,
}

impl Default for DeviceSize {
    fn default() -> Self {
        Self {
            text: 100,
            follow: false,
            interface: "cozy".into(),
        }
    }
}

/// What the device is told (`ConfigState::size`).
impl From<&DeviceSize> for wado_protocol::SizePrefs {
    fn from(s: &DeviceSize) -> Self {
        Self {
            text: s.text,
            follow: s.follow,
            interface: s.interface.clone(),
        }
    }
}

fn text<'de, D: Deserializer<'de>>(d: D) -> Result<u32, D::Error> {
    let n = u32::deserialize(d)?;
    if (TEXT_MIN..=TEXT_MAX).contains(&n) && n % TEXT_STEP == 0 {
        Ok(n)
    } else {
        Err(serde::de::Error::custom(format!(
            "text is a percentage from {TEXT_MIN} to {TEXT_MAX} in steps of {TEXT_STEP}, not {n}"
        )))
    }
}

fn interface<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    let s = String::deserialize(d)?;
    if INTERFACES.contains(&s.as_str()) {
        Ok(s)
    } else {
        Err(serde::de::Error::custom(format!(
            "interface is one of {}, not \"{s}\"",
            INTERFACES.join(", ")
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn written_under_its_device() {
        let c = crate::kdl::parse(
            "device \"k\" { size { text 120; follow #true; interface \"large\" } }",
        )
        .unwrap();
        let s = c.device["k"].size.clone().unwrap();
        assert_eq!(
            (s.text, s.follow, s.interface.as_str()),
            (120, true, "large")
        );
    }

    #[test]
    fn defaults_are_the_designs_own_size() {
        let s = DeviceSize::default();
        assert_eq!(
            (s.text, s.follow, s.interface.as_str()),
            (100, false, "cozy")
        );
        let c = crate::kdl::parse("device \"k\" { size { } }").unwrap();
        assert_eq!(c.device["k"].size.clone().unwrap(), s);
    }

    #[test]
    fn the_device_is_told_every_field() {
        let p = wado_protocol::SizePrefs::from(&DeviceSize::default());
        assert_eq!(
            serde_json::to_value(&p).unwrap(),
            serde_json::json!({ "text": 100, "follow": false, "interface": "cozy" })
        );
    }

    #[test]
    fn a_size_off_the_scale_is_an_error() {
        let e = crate::kdl::parse("device \"k\" {\n  size {\n    text 112\n  }\n}").unwrap_err();
        assert!(e.message.contains("text"), "{e}");
        assert_eq!(e.line, 3, "{e}");
        assert!(crate::kdl::parse("device \"k\" { size { text 80 } }").is_err());
        assert!(crate::kdl::parse("device \"k\" { size { text 155 } }").is_err());
        assert!(crate::kdl::parse("device \"k\" { size { interface \"huge\" } }").is_err());
    }
}
