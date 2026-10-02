//! One open shell, as a tab sees it.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShellInfo {
    pub id: u32,
    /// `zsh`, or the ssh alias.
    pub title: String,
    /// The ssh alias, for an ssh shell.
    #[serde(default)]
    pub host: Option<String>,
    /// Still running. An exited shell keeps its tab and output until closed.
    pub alive: bool,
}
