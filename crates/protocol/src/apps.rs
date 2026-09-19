//! Installed-application discovery: what the server found that can be launched.
//!
//! Deliberately thin. A desktop entry carries categories, MIME associations, actions and
//! localised names; none of that is needed to put a name in a list and a command behind it,
//! and every field here has to survive two transports.
//!
//! The icon is the one exception, and it is carried *inline* as a `data:` URI rather than as
//! a name or a path. Two reasons, both structural: relay mode has no HTTP route back to the
//! server, so a second fetch per icon would mean a new request/response pair on the relay
//! protocol; and the client is a browser that cannot read the server's filesystem, so a path
//! would be unusable even in direct mode.

use serde::{Deserialize, Serialize};

/// One launchable application, as discovered from an XDG desktop entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppEntry {
    /// Human-readable name (`Name=`), for display and filtering.
    pub name: String,
    /// The command to run (`Exec=`, with field codes removed).
    pub exec: String,
    /// The icon as a `data:` URI, when one was found and was small enough to carry.
    ///
    /// `None` is normal, not an error — plenty of entries name an icon no installed theme
    /// provides. The client draws a letter tile for those.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    /// Whether this application is running in the session right now.
    ///
    /// Not discovery — the desktop file cannot know it. It is filled in when the list is
    /// *answered*, from the commands the compositor still has processes for, which is why it
    /// lives on this type rather than in a second list the client would have to join itself.
    ///
    /// "Running" means the process is alive, not that it has a window. See
    /// `wado_compositor::headless::running_apps` for why that trade was made.
    #[serde(default)]
    pub running: bool,
    /// Installed, but not meant to appear in a menu: `NoDisplay=true` (MIME handlers, setup
    /// helpers, per-scheme stubs) or `Hidden=true` (deleted, per the spec).
    ///
    /// Carried rather than filtered out at the source, because "not normally listed" and "not
    /// launchable" are different claims and only the first one is true. On a normal desktop
    /// this is *half* of the desktop files installed — see the drawer's eye toggle, which is
    /// the one place they are wanted.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub hidden: bool,
}
