//! `security { }` — trust *policy*. The trusted-device list itself stays in
//! `~/.config/wado/trusted_clients`: it is state written at runtime by approvals and shared by a
//! pool, and a config file that the daemon rewrites on every approval is not a config file.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Security {
    /// While the trust list is empty, trust the first device that connects. Off means every
    /// device — the first included — must be added to `trusted_clients` by hand.
    pub trust_first_device: bool,
    /// The client key of the device allowed to change privileged settings from the client.
    /// Unset: the first device in `trusted_clients`.
    pub owner: Option<String>,
}

impl Default for Security {
    fn default() -> Self {
        Self {
            trust_first_device: true,
            owner: None,
        }
    }
}
