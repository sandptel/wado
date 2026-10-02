//! `shells { }` — the console's terminals.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Shells {
    /// Off refuses every shell request — for a host that streams apps but should not hand out
    /// a terminal.
    pub enabled: bool,
    /// The shell to run, as a login shell. Unset means `$SHELL`, else `/bin/sh`.
    pub program: Option<String>,
}

impl Default for Shells {
    fn default() -> Self {
        Self {
            enabled: true,
            program: None,
        }
    }
}
