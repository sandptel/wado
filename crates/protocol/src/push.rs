//! Web Push from the computer to a device (Decision Log `2026-10-30`, F3): which kinds it
//! wants, and the subscription its service worker made.

use serde::{Deserialize, Serialize};

const fn yes() -> bool {
    true
}

/// The kinds a device may turn on or off. All on by default (user, `2026-10-30`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PushKinds {
    /// Claude asks permission to do something.
    #[serde(default = "yes")]
    pub claude_permission: bool,
    /// Claude finished what it was asked.
    #[serde(default = "yes")]
    pub claude_done: bool,
    /// The computer is back online.
    #[serde(default = "yes")]
    pub online: bool,
    /// A big transfer finished.
    #[serde(default = "yes")]
    pub transfer: bool,
}

impl Default for PushKinds {
    fn default() -> Self {
        Self {
            claude_permission: true,
            claude_done: true,
            online: true,
            transfer: true,
        }
    }
}

/// What the computer says about push for this device.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PushInfo {
    /// The computer's VAPID public key (base64url, uncompressed P-256), for `subscribe()`.
    pub key: String,
    /// This device has a subscription on file here.
    pub subscribed: bool,
    pub kinds: PushKinds,
    /// Why the last subscribe or test failed; empty when it did not.
    #[serde(default)]
    pub error: String,
}

/// A `PushSubscription`, as `toJSON()` gives it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PushSub {
    pub endpoint: String,
    /// The browser's P-256 key, base64url.
    pub p256dh: String,
    /// Its 16-byte auth secret, base64url.
    pub auth: String,
}
