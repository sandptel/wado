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
    /// Touch frames are played out on a steady cadence, interpolated, rather than written the
    /// moment they land — libinput reads gesture speed from the time between events, so
    /// network jitter would otherwise read as a hand speeding up and slowing down. Off writes
    /// direct frames as they arrive; relayed frames are always buffered.
    pub smoothing: bool,
    /// The cadence frames are written at, like a real touchpad's report rate. Clamped 60..=250.
    pub rate_hz: u32,
    /// How far behind the phone the direct path plays, to absorb jitter (the relay path plays
    /// at least 24 ms behind). Clamped 0..=50.
    pub direct_delay_ms: u64,
}

impl Default for Trackpad {
    fn default() -> Self {
        Self {
            enabled: true,
            px_per_mm: 6,
            finger_silence_ms: 400,
            key_silence_ms: 5000,
            smoothing: true,
            rate_hz: 125,
            direct_delay_ms: 8,
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

    #[test]
    fn pacing_keys_default_and_parse() {
        let d = super::Trackpad::default();
        assert_eq!((d.smoothing, d.rate_hz, d.direct_delay_ms), (true, 125, 8));
        let c = crate::kdl::parse(
            "trackpad {\n  smoothing #false\n  rate-hz 200\n  direct-delay-ms 16\n}\n",
        )
        .unwrap();
        assert_eq!(
            (
                c.trackpad.smoothing,
                c.trackpad.rate_hz,
                c.trackpad.direct_delay_ms
            ),
            (false, 200, 16)
        );
    }
}
