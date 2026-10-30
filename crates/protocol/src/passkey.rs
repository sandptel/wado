//! The passkey step-up, as the device and the daemon speak it over the sealed relay link.
//!
//! Shells, writing files and the trackpad need a fresh unlock with this device's passkey
//! (Decision Log `2026-10-30`, F3). The WebAuthn bytes travel base64url, as the browser hands
//! them over; the daemon checks them (`server::passkey`).

use serde::{Deserialize, Serialize};

/// Where this device stands with its passkey on this computer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PasskeyState {
    /// A passkey is on file for this device.
    pub registered: bool,
    /// Unlocked now: shells, file writes and the trackpad are open to it.
    pub unlocked: bool,
    /// Seconds of idle left before it locks itself; 0 when locked.
    #[serde(default)]
    pub idle_left_s: u32,
    /// The idle window, so the device can run the same clock.
    #[serde(default)]
    pub idle_s: u32,
    /// Why the last register or unlock failed, in plain words; empty when it did not.
    #[serde(default)]
    pub error: String,
    /// The site the passkey was made for (`https://host`), so the device can tell a passkey
    /// made on another copy of the app from none at all.
    #[serde(default)]
    pub origin: String,
    /// How long an unlock lasts, as this device chose it: one of [`STAYS`]' names. Empty from a
    /// daemon older than the choice (`2026-10-30`): 15 minutes, and nothing to choose.
    #[serde(default)]
    pub stay: String,
    /// Lock the moment the app goes to the background. On unless the device turned it off.
    #[serde(default = "default_lock_on_leave")]
    pub lock_on_leave: bool,
}

impl Default for PasskeyState {
    fn default() -> Self {
        Self {
            registered: false,
            unlocked: false,
            idle_left_s: 0,
            idle_s: 0,
            error: String::new(),
            origin: String::new(),
            stay: String::new(),
            lock_on_leave: true,
        }
    }
}

/// `lock_on_leave` when a message leaves it out: on.
pub fn default_lock_on_leave() -> bool {
    true
}

/// The unlock lifetimes a device may choose, with the idle seconds each allows; 0 is "until I
/// lock it". The first is the default, and what a device that never chose gets.
pub const STAYS: &[(&str, u32)] = &[
    (STAY_DEFAULT, 15 * 60),
    ("1h", 60 * 60),
    ("8h", 8 * 60 * 60),
    (STAY_LOCK, 0),
];

/// The default lifetime: 15 minutes without use.
pub const STAY_DEFAULT: &str = "15m";

/// "Until I lock it": no idle clock at all.
pub const STAY_LOCK: &str = "lock";

/// The idle seconds `stay` allows (0: no idle lock), or `None` for a name not in [`STAYS`].
pub fn stay_idle_s(stay: &str) -> Option<u32> {
    STAYS.iter().find(|(n, _)| *n == stay).map(|(_, s)| *s)
}

/// What a `passkey_challenge` is for.
pub const REGISTER: &str = "register";
pub const UNLOCK: &str = "unlock";
