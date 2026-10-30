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

/// Where to deliver: a browser's `PushSubscription` (`toJSON()`'s fields), or a UnifiedPush
/// endpoint from the Android app. Both are RFC 8030 endpoints taking RFC 8291 (aes128gcm)
/// payloads with VAPID (RFC 8292), so the computer sends to them the same way.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PushSub {
    pub endpoint: String,
    /// The receiver's P-256 key, base64url (65 bytes, uncompressed).
    pub p256dh: String,
    /// Its 16-byte auth secret, base64url.
    pub auth: String,
    /// [`SUB_WEB`] (the default) or [`SUB_UNIFIEDPUSH`] — told apart for the record only.
    #[serde(default)]
    pub kind: String,
}

pub const SUB_WEB: &str = "web";
pub const SUB_UNIFIEDPUSH: &str = "unifiedpush";

/// The kinds of notification, as [`PushNote::kind`] names them.
pub const NOTE_CLAUDE_PERMISSION: &str = "claude_permission";
pub const NOTE_CLAUDE_DONE: &str = "claude_done";
pub const NOTE_ONLINE: &str = "online";
pub const NOTE_TRANSFER: &str = "transfer";
pub const NOTE_TEST: &str = "test";

/// What one push carries, decrypted — the same for the web app's service worker and a native
/// app, so both render the same notification with the same buttons.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PushNote {
    /// One of the `NOTE_*` names.
    pub kind: String,
    pub title: String,
    #[serde(default)]
    pub body: String,
    /// The computer's Remote ID (digits), so a tap opens the right one.
    pub computer: String,
    /// Its name, for the title of a list of them.
    #[serde(default)]
    pub computer_name: String,
    /// A later notification with the same tag replaces this one.
    #[serde(default)]
    pub tag: String,
    /// Buttons, in order. A tap on the body itself means "open".
    #[serde(default)]
    pub actions: Vec<PushAction>,
    /// For the Claude kinds: its session, and the question an action answers.
    #[serde(default)]
    pub session: String,
    #[serde(default)]
    pub ask: String,
    /// Unix seconds when it was sent; a stale question is not offered for an answer.
    #[serde(default)]
    pub at: u64,
}

/// A notification button. `id` is what the app acts on: [`ACTION_ALLOW`], [`ACTION_DENY`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PushAction {
    pub id: String,
    pub title: String,
}

pub const ACTION_ALLOW: &str = "allow";
pub const ACTION_DENY: &str = "deny";
