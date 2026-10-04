//! What can be configured — one file per `config.kdl` section.
//!
//! Every struct is `#[serde(default, deny_unknown_fields)]`: a missing key is the built-in
//! default, and a misspelt one is an error with a position rather than a setting that silently
//! does nothing.

pub mod binds;
pub mod de;
pub mod device;
pub mod files;
pub mod gestures;
pub mod input;
pub mod rules;
pub mod security;
pub mod server;
pub mod session;
pub mod shells;
pub mod stream;

use serde::{Deserialize, Serialize};

use std::collections::BTreeMap;

pub use binds::Binds;
pub use device::Device;
pub use files::Files;
pub use input::Input;
pub use rules::WindowRule;
pub use security::Security;
pub use server::{Server, Turn};
pub use session::Session;
pub use shells::Shells;
pub use stream::Stream;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Config {
    pub server: Server,
    pub security: Security,
    pub stream: Stream,
    pub session: Session,
    pub shells: Shells,
    pub files: Files,
    pub input: Input,
    /// Keyed by the device's client key.
    pub device: BTreeMap<String, Device>,
    pub binds: Binds,
    /// `"swipe-3-up"` → `"app-drawer"`. See [`gestures`].
    pub gestures: BTreeMap<String, String>,
    pub window_rule: Vec<WindowRule>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            server: Server::default(),
            security: Security::default(),
            stream: Stream::default(),
            session: Session::default(),
            shells: Shells::default(),
            files: Files::default(),
            input: Input::default(),
            device: BTreeMap::new(),
            binds: Binds::default(),
            gestures: gestures::defaults(),
            window_rule: Vec::new(),
        }
    }
}

impl Config {
    /// What serde cannot check: bind combos and action names. `(key path, message)`.
    pub fn check(&self) -> Result<(), (String, String)> {
        self.binds
            .parsed()
            .map_err(|(k, m)| (format!("binds.{k}"), m))?;
        gestures::check(&self.gestures).map_err(|(k, m)| (format!("gestures.{k}"), m))
    }
}

/// Node names that may repeat and collect into a list, in any section.
pub const LIST_NODES: &[&str] = &["autostart", "window-rule", "root", "deny"];

/// Node names whose first argument is a key: `device "abc" { … }` → `device.abc`.
pub const KEYED_NODES: &[&str] = &["device"];
