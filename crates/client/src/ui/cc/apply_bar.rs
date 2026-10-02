//! The pending-changes bar: shown only while the settings differ from the running session.
//!
//! Counted by comparing the session config these settings would build against the one last
//! sent, so it cannot disagree with what Apply will actually send.

use dioxus::prelude::*;

use crate::{actions, cfg, state::Ui};

/// How many top-level fields differ between two session configs (as JSON).
fn pending(applied: &str, now: &wado_protocol::SessionConfig) -> usize {
    let (Ok(a), Ok(b)) = (
        serde_json::from_str::<serde_json::Value>(applied),
        serde_json::to_value(now),
    ) else {
        return 0;
    };
    match (a.as_object(), b.as_object()) {
        (Some(a), Some(b)) => b.iter().filter(|(k, v)| a.get(*k) != Some(v)).count(),
        _ => 0,
    }
}

pub fn render(ui: Ui) -> Element {
    let applied = (ui.live.applied)();
    if !(ui.live.session_on)() || applied.is_empty() {
        return rsx! {};
    }
    let n = pending(&applied, &cfg::build(ui));
    if n == 0 {
        return rsx! {};
    }
    rsx! {
        div { class: "applybar",
            span { if n == 1 { "1 change" } else { "{n} changes" } }
            small { "Apps keep running" }
            button { class: "btn primary", onclick: move |_| actions::apply(ui), "Apply" }
        }
    }
}
