//! Every action shows its state: the Rust half of `js/pending.js` (Decision Log 2026-10-30).
//!
//! A button that asks the computer for something builds an [`Act`] — its id, the words for each
//! stage, which answer on the wire settles it — and [`run`]s it with the JS that sends the
//! request. The page owns the stages (pressed, working, long, outcome) and the timers, because
//! they must keep running however busy the WASM side is; this side only describes the action and
//! keeps what Try again and Cancel do when that is Rust code.
//!
//! The request is sent only if the action was not already under way: a second tap while it works
//! sends nothing. The JS gets the action's `ack` number as `a.ack`, for the request to carry.

// The Rust half is built and tested; wiring it into each action is the handover's next step.
#![allow(dead_code)]

use std::{cell::RefCell, collections::HashMap, rc::Rc};

use serde_json::{json, Value};

use crate::{bridge, state::Ui};

thread_local! {
    static RETRY: RefCell<HashMap<String, Rc<dyn Fn()>>> = RefCell::new(HashMap::new());
    static CANCEL: RefCell<HashMap<String, Rc<dyn Fn()>>> = RefCell::new(HashMap::new());
}

/// One action, as the page needs to know it.
#[derive(Clone, Debug, PartialEq)]
pub struct Act {
    spec: serde_json::Map<String, Value>,
}

impl Act {
    /// `id` names the action (`session.stop`, `launch:firefox`, `host:wifi`); `label` is what the
    /// control says while it works ("Closing…").
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        let mut spec = serde_json::Map::new();
        spec.insert("id".into(), Value::String(id.into()));
        spec.insert("label".into(), Value::String(label.into()));
        Act { spec }
    }
    fn set(mut self, k: &str, v: Value) -> Self {
        self.spec.insert(k.into(), v);
        self
    }
    /// The action in a few words, for "<what> didn't work: <why>" ("End the session").
    pub fn what(self, s: &str) -> Self {
        self.set("what", json!(s))
    }
    /// Who answers: "Still waiting for <wait> to answer…". Defaults to the computer's name.
    pub fn wait(self, s: &str) -> Self {
        self.set("wait", json!(s))
    }
    /// The answer that settles it well: `type`, or `type?key=value`; `$ack` is this action's ack.
    pub fn ok(self, m: &[&str]) -> Self {
        self.set("ok", json!(m))
    }
    /// The answer that refuses it; its `message` is the reason shown.
    pub fn fail(self, m: &[&str]) -> Self {
        self.set("fail", json!(m))
    }
    /// An answer that means "taken, now it is the app's turn" (a launch accepted, its window to
    /// come), with what the control says from then on.
    pub fn accept(self, m: &[&str], label: &str) -> Self {
        self.set("accept", json!(m))
            .set("acceptLabel", json!(label))
    }
    /// After an accept, what a long wait says — and that no window by the timeout is still fine.
    pub fn accepted_wait(self, long: &str, ok: &str) -> Self {
        self.set("acceptLong", json!(long))
            .set("acceptedOk", json!(ok))
    }
    /// What success says: in the live region, and on a card when it lands off-screen.
    pub fn done(self, s: &str) -> Self {
        self.set("done", json!(s))
    }
    /// Success says nothing on a card even off-screen (the change is its own confirmation).
    pub fn quiet(self) -> Self {
        self.set("quiet", json!(true))
    }
    /// A toggle: an answer under 300 ms shows no check — the toggle flipping is the confirmation.
    pub fn no_check(self) -> Self {
        self.set("check", json!(false))
    }
    /// The long-wait threshold and the give-up time, in ms.
    pub fn times(self, long_ms: u32, timeout_ms: u32) -> Self {
        self.set("longMs", json!(long_ms))
            .set("timeoutMs", json!(timeout_ms))
    }
    /// The answer is one only a daemon with acks sends: an older daemon is taken at its word.
    pub fn fresh(self) -> Self {
        self.set("fresh", json!(true))
    }
    /// The control, when it is not the one tapped or marked `data-act` (a CSS selector).
    pub fn el(self, sel: &str) -> Self {
        self.set("el", json!(sel))
    }
    pub fn id(&self) -> &str {
        self.spec.get("id").and_then(|v| v.as_str()).unwrap_or("")
    }
    fn json(&self) -> String {
        Value::Object(self.spec.clone()).to_string()
    }
}

/// Start `act` and, if it was not already under way, run `send` — JS that sends the request and
/// may use `a.ack`. `retry` is what Try again does (usually the same call again).
pub fn run(act: Act, send: String, retry: Option<Rc<dyn Fn()>>) {
    let id = act.id().to_string();
    let rust = retry.is_some();
    if let Some(r) = retry {
        RETRY.with(|m| m.borrow_mut().insert(id.clone(), r));
    }
    let mut act = act;
    if rust {
        act = act.set("rust", json!(true));
    }
    bridge::call(format!(
        "(() => {{ const W = window.__wado; const a = W.pending.start({spec}); if (!a) return; {send} }})();",
        spec = act.json(),
    ));
}

/// Like [`run`], with Cancel doing `cancel` (an action that can be called off).
pub fn run_cancellable(act: Act, send: String, retry: Rc<dyn Fn()>, cancel: Rc<dyn Fn()>) {
    CANCEL.with(|m| m.borrow_mut().insert(act.id().to_string(), cancel));
    run(act, send, Some(retry));
}

/// The page's Try again / Cancel for an action whose code is Rust (`bridge.rs` routes them here).
pub fn on_event(kind: &str, id: &str) {
    let f = match kind {
        "pendingRetry" => RETRY.with(|m| m.borrow().get(id).cloned()),
        "pendingCancel" => CANCEL.with(|m| m.borrow().get(id).cloned()),
        _ => None,
    };
    if let Some(f) = f {
        f();
    }
}

/// Settle an action from Rust: the answer arrived through a path the page does not see.
pub fn ok(id: &str) {
    bridge::call(format!("window.__wado.pending.ok({});", bridge::js(&id)));
}

/// The computer's name, for "Still waiting for <name> to answer…" in the page's own actions.
pub fn use_name(ui: Ui) {
    dioxus::prelude::use_effect(move || {
        let name = crate::profile::current_name(ui);
        bridge::call(format!(
            "window.__wado.computerName = {};",
            bridge::js(&name)
        ));
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_act_is_the_spec_the_page_reads() {
        let a = Act::new("session.stop", "Ending…")
            .what("End the session")
            .ok(&["session_stopped?ack=$ack"])
            .fail(&["session_error?ack=$ack"])
            .times(4000, 15000);
        let v: Value = serde_json::from_str(&a.json()).unwrap();
        assert_eq!(v["id"], "session.stop");
        assert_eq!(v["label"], "Ending…");
        assert_eq!(v["ok"][0], "session_stopped?ack=$ack");
        assert_eq!(v["longMs"], 4000);
        assert_eq!(a.id(), "session.stop");
    }
}
