//! One hook event, read from what Claude Code hands its hooks on stdin (wrapped by
//! `wado claude-event` with what only it can see: the tmux session, the ask id).

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// What `wado claude-event` sends a daemon: Claude's own JSON, and what the hook added.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Envelope {
    /// For a permission request: one id for this question, the same on every daemon.
    #[serde(default)]
    pub ask: String,
    /// The tmux session Claude runs in, when it is one wado made (`wado-…`).
    #[serde(default)]
    pub tmux: String,
    /// `WADO_CLAUDE` from Claude's environment: a session wado started without tmux.
    #[serde(default)]
    pub label: String,
    pub event: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Kind {
    Start,
    Prompt(String),
    /// Wants to use `tool` to do `detail`.
    Permission {
        tool: String,
        detail: String,
    },
    /// `notification_type`, message.
    Notify(String, String),
    /// Finished a turn, with its last words.
    Stop(String),
    End,
    /// Anything else: still proof of life.
    Other,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Hook {
    pub session: String,
    pub cwd: String,
    pub kind: Kind,
}

/// One line, at most `n` characters, for a card.
pub fn trim(s: &str, n: usize) -> String {
    let flat: String = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= n {
        return flat;
    }
    let mut out: String = flat.chars().take(n.saturating_sub(1)).collect();
    out.push('…');
    out
}

fn text(v: &Value, k: &str) -> String {
    v.get(k).and_then(Value::as_str).unwrap_or("").to_string()
}

/// What a tool wants to do, in a line: the command, the file, the URL — else its input.
fn detail(input: &Value) -> String {
    for k in [
        "command",
        "file_path",
        "path",
        "url",
        "pattern",
        "description",
    ] {
        if let Some(s) = input.get(k).and_then(Value::as_str) {
            return trim(s, 300);
        }
    }
    trim(&input.to_string(), 300)
}

pub fn parse(event: &Value) -> Option<Hook> {
    let session = text(event, "session_id");
    if session.is_empty() {
        return None;
    }
    let kind = match text(event, "hook_event_name").as_str() {
        "SessionStart" => Kind::Start,
        "UserPromptSubmit" => Kind::Prompt(trim(
            &[text(event, "prompt"), text(event, "prompt_text")].concat(),
            160,
        )),
        "PermissionRequest" => Kind::Permission {
            tool: text(event, "tool_name"),
            detail: detail(event.get("tool_input").unwrap_or(&Value::Null)),
        },
        "Notification" => Kind::Notify(
            text(event, "notification_type"),
            trim(&text(event, "message"), 200),
        ),
        "Stop" => Kind::Stop(trim(&text(event, "last_assistant_message"), 200)),
        "SessionEnd" => Kind::End,
        _ => Kind::Other,
    };
    Some(Hook {
        session,
        cwd: text(event, "cwd"),
        kind,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn reads_the_events_wado_listens_to() {
        let p = parse(
            &json!({"session_id":"s1","hook_event_name":"PermissionRequest","cwd":"/w",
            "tool_name":"Bash","tool_input":{"command":"rm -rf  build\n && ls"}}),
        )
        .unwrap();
        assert_eq!(p.cwd, "/w");
        assert_eq!(
            p.kind,
            Kind::Permission {
                tool: "Bash".into(),
                detail: "rm -rf build && ls".into()
            }
        );
        let s = parse(
            &json!({"session_id":"s1","hook_event_name":"Stop","last_assistant_message":"Done."}),
        );
        assert_eq!(s.unwrap().kind, Kind::Stop("Done.".into()));
        let n = parse(&json!({"session_id":"s1","hook_event_name":"Notification",
            "notification_type":"idle_prompt","message":"Claude is waiting for your input"}));
        assert_eq!(
            n.unwrap().kind,
            Kind::Notify(
                "idle_prompt".into(),
                "Claude is waiting for your input".into()
            )
        );
        assert!(
            parse(&json!({"hook_event_name":"Stop"})).is_none(),
            "no session"
        );
    }

    #[test]
    fn long_text_is_cut_to_a_line() {
        assert_eq!(trim("a\n b", 10), "a b");
        assert_eq!(trim("abcdefghij", 5), "abcd…");
    }
}
