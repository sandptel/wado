//! `device "<client key>" { }` — one device's own preferences, so they follow it across
//! browsers and survive a cleared cache.
//!
//! `prefs` is the client's settings blob, verbatim. wado never reads inside it: the client owns
//! its own settings' shape, and typing forty UI knobs into the daemon's schema would make every
//! client change a daemon change.
//!
//! ponytail: an opaque JSON string. Typed keys are the upgrade if people start hand-editing it.
//!
//! `trackpad` is that upgrade for the trackpad screen: typed, because it is meant to be read and
//! edited in the file, `wado tui` and `wado devices` as well as from the phone. `console` is the
//! same for how a session opens (console or full screen) and the deck's input face.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Device {
    /// What the device calls itself, for a human reading the file.
    pub name: Option<String>,
    pub prefs: Option<String>,
    pub trackpad: Option<super::DeviceTrackpad>,
    /// How a session opens on it, and the deck's face (the Pocket Console).
    pub console: Option<super::DeviceConsole>,
}
