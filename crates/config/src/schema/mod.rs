//! What can be configured — one file per `config.kdl` section.
//!
//! Every struct is `#[serde(default, deny_unknown_fields)]`: a missing key is the built-in
//! default, and a misspelt one is an error with a position rather than a setting that silently
//! does nothing.

pub mod de;
pub mod device;
pub mod input;
pub mod security;
pub mod server;
pub mod session;
pub mod shells;
pub mod stream;

use serde::{Deserialize, Serialize};

use std::collections::BTreeMap;

pub use device::Device;
pub use input::Input;
pub use security::Security;
pub use server::{Server, Turn};
pub use session::Session;
pub use shells::Shells;
pub use stream::Stream;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Config {
    pub server: Server,
    pub security: Security,
    pub stream: Stream,
    pub session: Session,
    pub shells: Shells,
    pub input: Input,
    /// Keyed by the device's client key.
    pub device: BTreeMap<String, Device>,
}

/// Node names that may repeat and collect into a list, in any section.
pub const LIST_NODES: &[&str] = &["autostart"];

/// Node names whose first argument is a key: `device "abc" { … }` → `device.abc`.
pub const KEYED_NODES: &[&str] = &["device"];
