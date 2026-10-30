//! The cards: each Claude session's state, moved by its hooks, and the permission requests
//! waiting for an answer.
//!
//! ```text
//!   SessionStart ─▶ waiting      UserPromptSubmit ─▶ working      PermissionRequest ─▶ permission
//!   answered ─▶ working          Notification(idle) ─▶ waiting    Stop ─▶ done      SessionEnd ─▶ ended
//! ```

use std::collections::HashMap;
use std::sync::mpsc::{Receiver, Sender, channel};

use wado_protocol::claude::{ClaudeAsk, ClaudeSession};

use super::event::{Envelope, Hook, Kind};

/// More cards than this is a runaway, not a person: the oldest go first.
pub const MAX_CARDS: usize = 24;
/// An ended session's card stays this long, so "it finished" can be read.
pub const ENDED_KEEP_S: u64 = 30 * 60;

/// What an event means beyond the card — for a notification.
#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    None,
    /// Asked permission: (session, ask).
    Asked(ClaudeSession, ClaudeAsk),
    /// Finished a turn.
    Finished(ClaudeSession),
}

#[derive(Default)]
pub struct Board {
    pub sessions: Vec<ClaudeSession>,
    waiters: HashMap<String, Sender<bool>>,
    /// Shells this daemon opened on a session, by its tmux name or `WADO_CLAUDE` label.
    pub shells: HashMap<String, u32>,
}

fn project(cwd: &str) -> String {
    cwd.trim_end_matches('/')
        .rsplit('/')
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or("~")
        .to_string()
}

impl Board {
    fn card(&mut self, h: &Hook, env: &Envelope, now: u64) -> &mut ClaudeSession {
        let i = match self.sessions.iter().position(|s| s.id == h.session) {
            Some(i) => i,
            None => {
                self.sessions.push(ClaudeSession {
                    id: h.session.clone(),
                    state: "waiting".into(),
                    since: now,
                    ..Default::default()
                });
                self.sessions.len() - 1
            }
        };
        let c = &mut self.sessions[i];
        if !h.cwd.is_empty() {
            c.cwd = h.cwd.clone();
            c.project = project(&h.cwd);
        }
        if env.tmux.starts_with("wado-") {
            c.tmux = Some(env.tmux.clone());
        }
        c
    }

    /// Apply one hook. A permission request also hands back the receiver its hook waits on.
    pub fn apply(
        &mut self,
        env: &Envelope,
        h: &Hook,
        now: u64,
    ) -> (Effect, Option<Receiver<bool>>) {
        let mut rx = None;
        let mut waiter = None;
        let mut effect = Effect::None;
        let ask_id = env.ask.clone();
        let label = env.label.clone();
        let shells = self.shells.clone();
        let c = self.card(h, env, now);
        c.since = now;
        match &h.kind {
            Kind::Start => {
                c.state = "waiting".into();
                c.ask = None;
            }
            Kind::Prompt(p) => {
                c.state = "working".into();
                c.ask = None;
                if !p.is_empty() {
                    c.message = p.clone();
                }
            }
            Kind::Permission { tool, detail } => {
                let ask = ClaudeAsk {
                    id: if ask_id.is_empty() {
                        format!("{now}")
                    } else {
                        ask_id.clone()
                    },
                    tool: tool.clone(),
                    detail: detail.clone(),
                };
                c.state = "permission".into();
                c.message = format!("Wants to use {tool}");
                c.ask = Some(ask.clone());
                effect = Effect::Asked(c.clone(), ask.clone());
                if !ask_id.is_empty() {
                    let (tx, r) = channel();
                    rx = Some(r);
                    waiter = Some((ask.id, tx));
                }
            }
            Kind::Notify(kind, message) => {
                // A permission prompt is already on the card from its own request.
                if kind != "permission_prompt" {
                    if kind == "idle_prompt" && c.state != "permission" {
                        c.state = "waiting".into();
                    }
                    if !message.is_empty() {
                        c.message = message.clone();
                    }
                }
            }
            Kind::Stop(last) => {
                c.state = "done".into();
                c.ask = None;
                if !last.is_empty() {
                    c.message = last.clone();
                }
                effect = Effect::Finished(c.clone());
            }
            Kind::End => {
                c.state = "ended".into();
                c.ask = None;
            }
            Kind::Other => {}
        }
        let key = c.tmux.clone().unwrap_or(label);
        c.shell = shells.get(&key).copied();
        if let Some((id, tx)) = waiter {
            self.waiters.insert(id, tx);
        }
        self.prune(now);
        (effect, rx)
    }

    fn prune(&mut self, now: u64) {
        self.sessions
            .retain(|s| s.state != "ended" || now.saturating_sub(s.since) < ENDED_KEEP_S);
        self.sessions.sort_by(|a, b| b.since.cmp(&a.since));
        self.sessions.truncate(MAX_CARDS);
    }

    /// Answer `ask` of `session`. False when nobody is waiting for it any more (answered
    /// elsewhere, timed out, or not this question).
    pub fn answer(&mut self, session: &str, ask: &str, allow: bool) -> bool {
        let Some(c) = self.sessions.iter_mut().find(|s| s.id == session) else {
            return false;
        };
        if c.ask.as_ref().is_none_or(|a| a.id != ask) {
            return false;
        }
        c.ask = None;
        c.state = "working".into();
        c.message = if allow {
            "Allowed from your phone"
        } else {
            "Denied from your phone"
        }
        .into();
        self.waiters
            .remove(ask)
            .is_some_and(|tx| tx.send(allow).is_ok())
    }

    /// The question was settled elsewhere (another daemon's device, the terminal, a timeout).
    pub fn resolved(&mut self, ask: &str) {
        self.waiters.remove(ask);
        for c in &mut self.sessions {
            if c.ask.as_ref().is_some_and(|a| a.id == ask) {
                c.ask = None;
                c.state = "working".into();
            }
        }
    }

    /// A shell was opened on the session known as `key` (tmux name or label).
    pub fn shell_opened(&mut self, key: &str, id: u32) {
        self.shells.insert(key.to_string(), id);
        for c in &mut self.sessions {
            if c.tmux.as_deref() == Some(key) {
                c.shell = Some(id);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::event::parse;
    use super::*;
    use serde_json::json;

    fn env(ask: &str, v: serde_json::Value) -> (Envelope, Hook) {
        let e = Envelope {
            ask: ask.into(),
            tmux: "wado-api".into(),
            label: String::new(),
            event: v.clone(),
        };
        (e, parse(&v).unwrap())
    }

    #[test]
    fn hooks_move_the_card() {
        let mut b = Board::default();
        let (e, h) = env(
            "",
            json!({"session_id":"s","hook_event_name":"SessionStart","cwd":"/home/me/api"}),
        );
        b.apply(&e, &h, 1);
        assert_eq!(
            (b.sessions[0].project.as_str(), b.sessions[0].state.as_str()),
            ("api", "waiting")
        );
        assert_eq!(b.sessions[0].tmux.as_deref(), Some("wado-api"));
        let (e, h) = env(
            "",
            json!({"session_id":"s","hook_event_name":"UserPromptSubmit","prompt":"fix the tests"}),
        );
        b.apply(&e, &h, 2);
        assert_eq!(
            (b.sessions[0].state.as_str(), b.sessions[0].message.as_str()),
            ("working", "fix the tests")
        );
        let (e, h) = env(
            "",
            json!({"session_id":"s","hook_event_name":"Stop","last_assistant_message":"All green."}),
        );
        let (fx, _) = b.apply(&e, &h, 3);
        assert!(matches!(fx, Effect::Finished(c) if c.message == "All green."));
        assert_eq!(b.sessions[0].state, "done");
        let (e, h) = env("", json!({"session_id":"s","hook_event_name":"SessionEnd"}));
        b.apply(&e, &h, 4);
        assert_eq!(b.sessions[0].state, "ended");
        b.prune(4 + ENDED_KEEP_S);
        assert!(b.sessions.is_empty(), "an ended card goes after a while");
    }

    #[test]
    fn allow_and_deny_reach_the_waiting_hook_once() {
        let mut b = Board::default();
        let (e, h) = env(
            "a1",
            json!({"session_id":"s","hook_event_name":"PermissionRequest",
            "tool_name":"Bash","tool_input":{"command":"cargo test"}}),
        );
        let (fx, rx) = b.apply(&e, &h, 1);
        let rx = rx.expect("a waiter");
        assert!(matches!(fx, Effect::Asked(_, ref a) if a.detail == "cargo test"));
        assert_eq!(b.sessions[0].state, "permission");
        assert!(!b.answer("s", "a0", true), "not this question");
        assert!(b.answer("s", "a1", true));
        assert_eq!(rx.recv().unwrap(), true);
        assert_eq!(b.sessions[0].state, "working");
        assert!(!b.answer("s", "a1", false), "answered already");

        let (e, h) = env(
            "a2",
            json!({"session_id":"s","hook_event_name":"PermissionRequest","tool_name":"Edit","tool_input":{"file_path":"/x"}}),
        );
        let (_, rx) = b.apply(&e, &h, 2);
        assert!(b.answer("s", "a2", false));
        assert_eq!(rx.unwrap().recv().unwrap(), false);

        let (e, h) = env(
            "a3",
            json!({"session_id":"s","hook_event_name":"PermissionRequest","tool_name":"Edit"}),
        );
        let (_, rx) = b.apply(&e, &h, 3);
        b.resolved("a3");
        assert!(
            rx.unwrap().recv().is_err(),
            "settled elsewhere: the waiter is let go"
        );
        assert!(b.sessions[0].ask.is_none());
    }
}
