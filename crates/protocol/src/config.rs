//! The daemon's config as one viewer sees it — what the control centre needs to grey out a
//! capped control, restore this device's settings, and say when the file is broken.

use serde::{Deserialize, Serialize};

use crate::EncoderBackend;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ConfigState {
    #[serde(default)]
    pub limits: HostLimits,
    /// Input settings the host pins; a set field overrides the client's own.
    #[serde(default)]
    pub input: PinnedInput,
    /// This device's saved settings blob (`device "<key>" { prefs … }`), if it has one.
    #[serde(default)]
    pub prefs: Option<String>,
    /// This device may change privileged settings.
    #[serde(default)]
    pub owner: bool,
    /// The file failed its last reload — `config.kdl:14:9: …`. The daemon is still running on
    /// the last good config.
    #[serde(default)]
    pub error: Option<String>,
    /// Sections changed in the file that only take effect after a daemon restart.
    #[serde(default)]
    pub restart: Vec<String>,
    /// What three-finger swipes do: `"swipe-3-up"` → `"app-drawer"`.
    #[serde(default)]
    pub gestures: std::collections::BTreeMap<String, String>,
    /// Keyboard shortcuts the compositor keeps: `"Mod+Q"` → `"close-window"`, and what `Mod` is.
    #[serde(default)]
    pub binds: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub bind_mod: String,
    /// How many window rules config.kdl has.
    #[serde(default)]
    pub window_rules: usize,
    /// Shells are allowed on this host.
    #[serde(default = "yes")]
    pub shells: bool,
}

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostLimits {
    pub max_fps: Option<u32>,
    pub max_bitrate: Option<u32>,
    pub max_width: Option<u32>,
    pub max_height: Option<u32>,
    /// Set when the host forces one encoder for everyone.
    pub encoder: Option<EncoderBackend>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PinnedInput {
    pub repeat_rate: Option<i32>,
    pub repeat_delay: Option<i32>,
    pub focus_follows_pointer: Option<bool>,
}
