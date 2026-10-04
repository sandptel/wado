//! `files { }` — what the file manager may reach (`WADO_PLAN.md`, Decision Log `2026-10-04`).
//!
//! Which *devices* may use it is not here: that is the `files=` grant on each device's line in
//! `trusted_clients` (`wado files grant`), state rather than policy, like the trust list itself.

use serde::{Deserialize, Serialize};

/// Always denied, whatever the config says. `~/.config/wado` above all: it holds the trust list
/// and the grants, so a device that could write there could grant itself more.
pub const ALWAYS_DENY: &[&str] = &[
    "~/.ssh",
    "~/.gnupg",
    "~/.config/wado",
    "~/.local/share/keyrings",
];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Files {
    /// Off refuses every file request.
    pub enabled: bool,
    /// The trees a device may browse, one `root` node each. `~` is home; `/` only if written.
    pub root: Vec<String>,
    /// Paths refused inside a root, on top of [`ALWAYS_DENY`].
    pub deny: Vec<String>,
    /// List dotfiles.
    pub hidden: bool,
    /// `"inside"`: a symlink is followed while it stays inside its root. `"never"`: never.
    pub follow_symlinks: String,
    /// Downloads while a session's video is live are held to this, in Mbps (0 = no cap). Halved
    /// while the viewer reports its decoder is strained.
    pub rate_with_video: u32,
    /// An upload is refused when it would leave less than this free, in MiB.
    pub reserve_mib: u64,
}

impl Default for Files {
    fn default() -> Self {
        Self {
            enabled: true,
            root: Vec::new(),
            deny: Vec::new(),
            hidden: false,
            follow_symlinks: "inside".into(),
            rate_with_video: 40,
            reserve_mib: 1024,
        }
    }
}

impl Files {
    /// The configured roots, or home when none is written.
    pub fn roots(&self) -> Vec<String> {
        if self.root.is_empty() {
            vec!["~".into()]
        } else {
            self.root.clone()
        }
    }
}
