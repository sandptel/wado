//! Claude Code's sessions on the selected computer, as the UI holds them (`js/claude.js` feeds
//! them; `ui::claude` draws them).

use dioxus::prelude::*;
use wado_protocol::claude::{ClaudeDirs, ClaudeState};

use crate::state::Ui;

/// The cards, as the selected computer last said them. `None` until it has.
pub static CARDS: GlobalSignal<Option<ClaudeState>> = Signal::global(|| None);
/// The new-session picker: open, and the folder it shows.
pub static PICKER: GlobalSignal<bool> = Signal::global(|| false);
pub static DIRS: GlobalSignal<Option<ClaudeDirs>> = Signal::global(|| None);

/// `{type:"claude"|"claudeDirs"|"claudeShell", …}` from the bridge.
pub fn on_event(ui: Ui, kind: &str, msg: &serde_json::Value) {
    match kind {
        "claude" => {
            *CARDS.write() = msg
                .get("state")
                .and_then(|v| serde_json::from_value(v.clone()).ok());
        }
        "claudeDirs" => {
            *DIRS.write() = msg
                .get("dirs")
                .and_then(|v| serde_json::from_value(v.clone()).ok());
        }
        "claudePicker" => {
            *DIRS.write() = None;
            *PICKER.write() = true;
        }
        // A session's shell is on its way: show the console on it.
        "claudeShell" => {
            *PICKER.write() = false;
            let mut live = ui.live;
            live.console_tab.set("shell".to_string());
            live.console_open.set(true);
        }
        _ => {}
    }
}

/// The state as words a person reads on a card.
pub fn words(state: &str) -> &'static str {
    match state {
        "working" => "Working",
        "permission" => "Needs you",
        "waiting" => "Waiting for you",
        "done" => "Done",
        "ended" => "Ended",
        _ => "Idle",
    }
}
