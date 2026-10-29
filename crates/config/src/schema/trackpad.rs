//! `trackpad { }` — a phone as this computer's touchpad and keyboard (`wado_server::pad`).
//! Which device may use it is a grant (`input`, in `trusted_clients`); this is how it behaves.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Trackpad {
    /// Off refuses every trackpad, whatever a device's grants say.
    pub enabled: bool,
    /// The phone's CSS pixels per millimetre of virtual touchpad. libinput accelerates by
    /// physical distance, so a larger number is a smaller pad and a slower pointer.
    pub px_per_mm: u32,
    /// A finger down with no frame for this long is lifted (the phone or its link is gone).
    pub finger_silence_ms: u64,
    /// No message at all for this long and held keys (a locked Super) are let go.
    pub key_silence_ms: u64,
}

impl Default for Trackpad {
    fn default() -> Self {
        Self {
            enabled: true,
            px_per_mm: 6,
            finger_silence_ms: 400,
            key_silence_ms: 5000,
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn written_in_config_kdl() {
        let c = crate::kdl::parse(
            "trackpad {\n  enabled #false\n  px-per-mm 8\n  finger-silence-ms 300\n}\n",
        )
        .unwrap();
        assert!(!c.trackpad.enabled);
        assert_eq!(
            (
                c.trackpad.px_per_mm,
                c.trackpad.finger_silence_ms,
                c.trackpad.key_silence_ms
            ),
            (8, 300, 5000)
        );
        assert!(
            crate::kdl::parse("trackpad { speed 2 }").is_err(),
            "a misspelt key is an error"
        );
    }
}
