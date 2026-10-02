//! `device "<client key>" { }` — one device's own preferences, so they follow it across
//! browsers and survive a cleared cache.
//!
//! `prefs` is the client's settings blob, verbatim. wado never reads inside it: the client owns
//! its own settings' shape, and typing forty UI knobs into the daemon's schema would make every
//! client change a daemon change.
//!
//! ponytail: an opaque JSON string. Typed keys are the upgrade if people start hand-editing it.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Device {
    /// What the device calls itself, for a human reading the file.
    pub name: Option<String>,
    pub prefs: Option<String>,
}
