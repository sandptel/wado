//! The passkey step-up, as the device and the daemon speak it over the sealed relay link.
//!
//! Shells, writing files and the trackpad need a fresh unlock with this device's passkey
//! (Decision Log `2026-10-30`, F3). The WebAuthn bytes travel base64url, as the browser hands
//! them over; the daemon checks them (`server::passkey`).

use serde::{Deserialize, Serialize};

/// Where this device stands with its passkey on this computer.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
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
}

/// What a `passkey_challenge` is for.
pub const REGISTER: &str = "register";
pub const UNLOCK: &str = "unlock";
