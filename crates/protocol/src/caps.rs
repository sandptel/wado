//! What this computer can do, by capability — the names the platform layer (`wado-platform`)
//! registers its providers under, and the set a client reads to hide what is missing here rather
//! than offer a button that does nothing.
//!
//! On the wire a capability is its [`Capability::name`], never an enum tag, so a client meeting a
//! name it does not know (a newer daemon) skips it instead of failing to read the set.

use serde::{Deserialize, Serialize};

/// One host capability. Each is one trait in `wado-platform`, provided or honestly absent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Capability {
    /// A multi-touch touchpad device (`TouchInjector`).
    Touch,
    /// A keyboard device: named keys and chords (`KeyInjector`).
    Keys,
    /// A game controller device (`GamepadInjector`).
    Gamepad,
    /// Typing committed Unicode text (`TextInjector`).
    Text,
    /// Outputs, streams, volume (`AudioMixer`).
    Audio,
    /// A sound output that plays to the viewer (`ViewerSink`).
    ViewerSink,
    /// Media players (`MediaPlayers`).
    Media,
    /// The screen's backlight (`Brightness`).
    Brightness,
    /// Wi-Fi (`WifiRadio`).
    Wifi,
    /// Bluetooth (`Bluetooth`).
    Bluetooth,
    /// Keeping the computer awake (`PowerInhibit`).
    PowerInhibit,
    /// Lock and sleep (`SystemPower`).
    SystemPower,
    /// The desktop's clipboard (`Clipboard`).
    Clipboard,
    /// The desktop's clipboard history (`ClipHistory`).
    ClipHistory,
    /// Windows and workspaces (`WindowManager`).
    WindowManager,
    /// Opening apps on the desktop (`Launcher`).
    Launcher,
    /// Screenshots of the desktop (`Screenshot`).
    Screenshot,
    /// What the computer is (`SysInfoSource`).
    SysInfo,
    /// The desktop's trash (`Trash`).
    Trash,
    /// File thumbnails (`Thumbnailer`).
    Thumbnailer,
    /// Shells on a pseudo-terminal (`PtySpawner`).
    Pty,
}

impl Capability {
    /// Every capability, in a fixed order.
    pub const ALL: [Capability; 21] = [
        Capability::Touch,
        Capability::Keys,
        Capability::Gamepad,
        Capability::Text,
        Capability::Audio,
        Capability::ViewerSink,
        Capability::Media,
        Capability::Brightness,
        Capability::Wifi,
        Capability::Bluetooth,
        Capability::PowerInhibit,
        Capability::SystemPower,
        Capability::Clipboard,
        Capability::ClipHistory,
        Capability::WindowManager,
        Capability::Launcher,
        Capability::Screenshot,
        Capability::SysInfo,
        Capability::Trash,
        Capability::Thumbnailer,
        Capability::Pty,
    ];

    /// The wire name: snake_case, stable.
    pub const fn name(self) -> &'static str {
        match self {
            Capability::Touch => "touch",
            Capability::Keys => "keys",
            Capability::Gamepad => "gamepad",
            Capability::Text => "text",
            Capability::Audio => "audio",
            Capability::ViewerSink => "viewer_sink",
            Capability::Media => "media",
            Capability::Brightness => "brightness",
            Capability::Wifi => "wifi",
            Capability::Bluetooth => "bluetooth",
            Capability::PowerInhibit => "power_inhibit",
            Capability::SystemPower => "system_power",
            Capability::Clipboard => "clipboard",
            Capability::ClipHistory => "clip_history",
            Capability::WindowManager => "window_manager",
            Capability::Launcher => "launcher",
            Capability::Screenshot => "screenshot",
            Capability::SysInfo => "sys_info",
            Capability::Trash => "trash",
            Capability::Thumbnailer => "thumbnailer",
            Capability::Pty => "pty",
        }
    }

    /// The capability named `name`, if this build knows it.
    pub fn from_name(name: &str) -> Option<Capability> {
        Capability::ALL.into_iter().find(|c| c.name() == name)
    }
}

/// What this computer has: the capabilities provided, and why each missing one is missing.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CapSet {
    /// Provided, by name.
    #[serde(default)]
    pub on: Vec<String>,
    /// Missing, by name, with the reason in plain words.
    #[serde(default)]
    pub off: Vec<CapOff>,
}

/// One missing capability and why.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CapOff {
    pub cap: String,
    pub why: String,
}

impl CapSet {
    /// `cap` is provided here.
    pub fn has(&self, cap: Capability) -> bool {
        self.on.iter().any(|n| n == cap.name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_unique_and_round_trip() {
        let mut names: Vec<_> = Capability::ALL.iter().map(|c| c.name()).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), Capability::ALL.len());
        for c in Capability::ALL {
            assert_eq!(Capability::from_name(c.name()), Some(c));
        }
        assert_eq!(Capability::from_name("teleport"), None);
    }

    #[test]
    fn a_set_reads_unknown_names_without_failing() {
        let s: CapSet = serde_json::from_str(
            r#"{"on":["audio","teleport"],"off":[{"cap":"wifi","why":"no NetworkManager"}]}"#,
        )
        .unwrap();
        assert!(s.has(Capability::Audio));
        assert!(!s.has(Capability::Wifi));
        assert_eq!(s.off[0].why, "no NetworkManager");
    }
}
