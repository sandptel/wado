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
    /// What happens to a device not on the trust list. `"ask"`: it waits for a connected device
    /// (or `wado approve`) to say once, always or deny. `"open"`: it is let in as if approved
    /// "once" — never added to the trust list, so no files, no shells, gone when set back to
    /// `"ask"`. Anyone who reaches this computer's remote id gets its desktop: for testing.
    /// Anything else reads as `"ask"`.
    pub join: String,
    /// What a newly trusted device may do beyond the desktop, as grant tokens: `files-ro`,
    /// `files-rw`, `shells`, `settings`, `host`. A pairing code's checklist starts from this;
    /// a device approved "always" gets exactly this. Files still need a QR pin to count.
    pub new_device: String,
    /// The client key of the device allowed to change privileged settings from the client.
    /// Unset: the first device in `trusted_clients`.
    pub owner: Option<String>,
}

impl Security {
    pub fn open_join(&self) -> bool {
        self.join == "open"
    }
}

impl Default for Security {
    fn default() -> Self {
        Self {
            trust_first_device: true,
            join: "ask".into(),
            new_device: "host".into(),
            owner: None,
        }
    }
}
