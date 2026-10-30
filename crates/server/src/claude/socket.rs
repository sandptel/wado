//! The daemon's end of `wado claude-event`, on its local command socket
//! (`crate::config::socket`): one event per connection, and for a permission request the
//! connection stays open until a phone answers.
//!
//! ```text
//!   claude <envelope json>   →  ok | allow | deny | none
//!   claude-resolved <ask>    →  ok      (answered on another daemon, or timed out)
//! ```

use std::io::Write;
use std::os::unix::net::UnixStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use super::board::Effect;
use super::event::{self, Envelope};

/// How long a permission request waits for a phone before Claude asks in its own terminal.
/// Under the hook's 600 s so the hook, not Claude, decides what a timeout means.
pub const WAIT: Duration = Duration::from_secs(300);

/// A device that may see the cards is connected to this daemon (`super::relay::follow`).
static PRESENT: AtomicBool = AtomicBool::new(false);

pub fn present(on: bool) {
    PRESENT.store(on, Ordering::SeqCst);
}

fn now_s() -> u64 {
    crate::passkey::now_s()
}

/// Serve one `claude …` request. Runs on its own thread: it may block for [`WAIT`].
pub fn serve(line: &str, mut stream: UnixStream) {
    let answer = handle(line);
    let _ = stream.write_all(format!("{answer}\n").as_bytes());
}

fn handle(line: &str) -> &'static str {
    if let Some(ask) = line.strip_prefix("claude-resolved ") {
        super::board().resolved(ask.trim());
        super::changed();
        return "ok";
    }
    let Some(json) = line.strip_prefix("claude ") else {
        return "error: unknown claude request";
    };
    let Ok(env) = serde_json::from_str::<Envelope>(json) else {
        return "error: not a claude event";
    };
    let Some(hook) = event::parse(&env.event) else {
        return "error: no session in that event";
    };
    let (effect, waiter) = super::board().apply(&env, &hook, now_s());
    super::changed();
    let asked = matches!(effect, Effect::Asked(..));
    crate::push::claude(&effect);
    let Some(rx) = waiter else {
        return "ok";
    };
    // Nobody here to ask: say so at once, and Claude asks in its terminal as it always has.
    if asked && !PRESENT.load(Ordering::SeqCst) && !crate::push::wants_claude_asks() {
        super::board().resolved(&env.ask);
        super::changed();
        return "none";
    }
    match rx.recv_timeout(WAIT) {
        Ok(true) => "allow",
        Ok(false) => "deny",
        Err(_) => {
            super::board().resolved(&env.ask);
            super::changed();
            "none"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_land_on_the_board_and_garbage_is_refused() {
        let id = format!("sock-{}", rand::random::<u32>());
        let line = format!(
            "claude {}",
            serde_json::json!({"event":{"session_id":id,"hook_event_name":"UserPromptSubmit","cwd":"/w/api","prompt":"go"}})
        );
        assert_eq!(handle(&line), "ok");
        assert!(
            super::super::board()
                .sessions
                .iter()
                .any(|s| s.id == id && s.state == "working")
        );
        assert!(handle("claude {").starts_with("error"));
        assert!(handle("claude {\"event\":{}}").starts_with("error"));
        assert_eq!(handle("claude-resolved nothing"), "ok");
    }

    #[test]
    fn a_permission_request_waits_for_the_answer_from_the_phone() {
        present(true);
        let id = format!("sock-ask-{}", rand::random::<u32>());
        let ask = format!("ask-{id}");
        let line = format!(
            "claude {}",
            serde_json::json!({"ask":ask,"event":{"session_id":id,"hook_event_name":"PermissionRequest",
                "tool_name":"Bash","tool_input":{"command":"ls"}}})
        );
        let t = std::thread::spawn(move || handle(&line));
        // The phone answers once the card is up.
        let (s, a) = (id.clone(), ask.clone());
        for _ in 0..200 {
            if super::super::board().answer(&s, &a, true) {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(t.join().unwrap(), "allow");
    }
}
