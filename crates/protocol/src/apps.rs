//! Installed-application discovery: what the server found that can be launched.
//!
//! Deliberately thin. A desktop entry carries icons, categories, MIME associations, actions
//! and localised names; none of that is needed to put a name in a list and a command behind
//! it, and every field here has to survive two transports.

use serde::{Deserialize, Serialize};

/// One launchable application, as discovered from an XDG desktop entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppEntry {
    /// Human-readable name (`Name=`), for display and filtering.
    pub name: String,
    /// The command to run (`Exec=`, with field codes removed).
    pub exec: String,
}
