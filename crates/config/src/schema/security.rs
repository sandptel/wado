//! `security { }` — trust *policy*. The trusted-device list itself stays in
//! `~/.config/wado/trusted_clients`: it is state written at runtime by pairing and shared by a
//! pool, and a config file that the daemon rewrites on every pairing is not a config file.
//!
//! A new device gets in only with a pairing code shown on the computer (`wado pair`, decided
//! `2026-10-27`). `trust-first-device` and `join` went with first-device-wins and remote
//! approval; they still parse, so an old config.kdl loads, and [`Security::deprecated`] names
//! them for the daemon to warn about.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Security {
    /// Removed — accepted and ignored. Was: trust the first device to connect.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trust_first_device: Option<bool>,
    /// Removed — accepted and ignored. Was: `"ask"` a connected device, or `"open"`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub join: Option<String>,
    /// What a newly trusted device may do beyond the desktop, as grant tokens: `files-ro`,
    /// `files-rw`, `shells`, `settings`, `host`, `input`. A pairing code's checklist starts from
    /// this. Files still need a QR pin to count.
    pub new_device: String,
    /// The client key of the device allowed to change privileged settings from the client.
    /// Unset: the first device in `trusted_clients`.
    pub owner: Option<String>,
}

#[cfg(test)]
mod tests {
    use crate::Config;

    #[test]
    fn an_old_config_with_the_removed_keys_still_loads_and_says_so() {
        let kdl = "security {\n    trust-first-device #false\n    join \"open\"\n    new-device \"host\"\n}\n";
        let c: Config = crate::kdl::parse(kdl).expect("old keys still parse");
        assert_eq!(
            c.security.deprecated(),
            vec!["security.trust-first-device", "security.join"]
        );
        assert!(Config::default().security.deprecated().is_empty());
    }
}

impl Security {
    /// The removed keys this config still sets — each ignored, for a one-line warning.
    pub fn deprecated(&self) -> Vec<&'static str> {
        let mut out = Vec::new();
        if self.trust_first_device.is_some() {
            out.push("security.trust-first-device");
        }
        if self.join.is_some() {
            out.push("security.join");
        }
        out
    }
}

impl Default for Security {
    fn default() -> Self {
        Self {
            trust_first_device: None,
            join: None,
            new_device: "host".into(),
            owner: None,
        }
    }
}
