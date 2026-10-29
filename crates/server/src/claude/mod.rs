//! Claude from the couch (F3, `2026-10-30`): Claude Code's sessions on this computer as status
//! cards on the phone, its permission requests answered from the card or the notification, and
//! new sessions started in a project folder.
//!
//! ```text
//!   claude ──hook──▶ wado claude-event ──socket──▶ every daemon ──▶ board ──▶ cards on the phone
//!                         ▲  blocks on a permission request          │
//!                         └──────────── allow / deny ◀───────────────┘ (card or notification)
//! ```
//!
//! | file | job |
//! |---|---|
//! | [`event`] | one hook's JSON, read |
//! | [`board`] | the cards and the questions waiting for an answer |
//! | [`socket`] | the daemon's end of `wado claude-event` |
//! | [`hook_cli`] | `wado claude-event` itself |
//! | [`hooks`] | `wado claude hooks install|uninstall`: wado's entries in Claude's settings |
//! | [`ojson`] | JSON that keeps its key order, for editing those settings |
//! | [`diff`] | the diff shown before they are written |
//! | [`spawn`] | a new session in a folder (tmux), a shell on a running one |
//! | [`dirs`] | the folders the new-session picker walks |
//! | [`relay`] | the messages over the relay link |
//!
//! Every daemon of a pool gets every event, so whichever one a phone is on shows the cards;
//! the hook takes the first answer from any of them.

pub mod board;
pub mod diff;
pub mod dirs;
pub mod event;
pub mod hook_cli;
pub mod hooks;
pub mod ojson;
pub mod relay;
pub mod socket;
pub mod spawn;

use std::path::PathBuf;
use std::sync::{LazyLock, Mutex, MutexGuard};

use tokio::sync::watch;
use wado_protocol::claude::ClaudeState;

static BOARD: LazyLock<Mutex<board::Board>> = LazyLock::new(Mutex::default);
static CHANGES: LazyLock<watch::Sender<u64>> = LazyLock::new(|| watch::channel(0).0);

pub fn board() -> MutexGuard<'static, board::Board> {
    BOARD.lock().unwrap_or_else(|e| e.into_inner())
}

/// Tell every follower the cards changed.
pub fn changed() {
    CHANGES.send_modify(|n| *n = n.wrapping_add(1));
}

pub fn changes() -> watch::Receiver<u64> {
    CHANGES.subscribe()
}

/// Claude Code's settings: `$CLAUDE_CONFIG_DIR/settings.json`, else `~/.claude/settings.json`.
pub fn settings_path() -> PathBuf {
    std::env::var_os("CLAUDE_CONFIG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::var_os("HOME")
                .map_or_else(|| PathBuf::from("/"), PathBuf::from)
                .join(".claude")
        })
        .join("settings.json")
}

/// Are wado's hooks in Claude's settings?
pub fn hooks_installed() -> bool {
    std::fs::read_to_string(settings_path())
        .ok()
        .and_then(|t| ojson::J::parse(&t).ok())
        .is_some_and(|s| hooks::installed(&s))
}

/// The cards, as a device sees them.
pub fn state() -> ClaudeState {
    ClaudeState {
        sessions: board().sessions.clone(),
        hooks: hooks_installed(),
        tmux: spawn::has_tmux(),
    }
}
