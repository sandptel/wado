//! Claude Code sessions on the computer, as status cards on the device (Decision Log
//! `2026-10-27` and `2026-10-30`, F3). Metadata only: reading them needs the `shells` grant and
//! no passkey; answering a permission request, or opening a session's shell, needs the passkey.

use serde::{Deserialize, Serialize};

/// One Claude session, from its hooks.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClaudeSession {
    /// Claude's own `session_id`.
    pub id: String,
    /// The project folder's name.
    pub project: String,
    /// Where it runs.
    pub cwd: String,
    /// `working` | `permission` | `waiting` | `done` | `ended`.
    pub state: String,
    /// The last thing it said or asked, trimmed.
    #[serde(default)]
    pub message: String,
    /// Unix seconds of the last change.
    pub since: u64,
    /// The permission it is waiting on, while `state` is `permission`.
    #[serde(default)]
    pub ask: Option<ClaudeAsk>,
    /// The tmux session wado started it in (`wado-<project>`), when it did.
    #[serde(default)]
    pub tmux: Option<String>,
    /// A shell of this daemon that shows it, when there is one.
    #[serde(default)]
    pub shell: Option<u32>,
}

/// A permission request a hook is holding open, waiting for an answer.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClaudeAsk {
    /// One per request; an answer names it, so a late answer cannot land on the next one.
    pub id: String,
    /// `Bash`, `Edit`, …
    pub tool: String,
    /// What it wants to do: the command, the file — trimmed.
    pub detail: String,
}

/// The cards, and what the computer can do about them.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClaudeState {
    pub sessions: Vec<ClaudeSession>,
    /// `wado claude hooks install` has been run: without it there are never any cards.
    pub hooks: bool,
    /// tmux is installed, so a new session survives the daemon and can be attached at the desk.
    pub tmux: bool,
}

/// A folder of the computer, for the new-session picker.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClaudeDirs {
    pub path: String,
    /// Empty at the top (the home folder).
    #[serde(default)]
    pub parent: String,
    pub dirs: Vec<String>,
    /// Folders Claude was used in lately, for one tap.
    #[serde(default)]
    pub recent: Vec<String>,
    #[serde(default)]
    pub error: String,
}
