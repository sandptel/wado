//! Running sessions on this computer — on whichever daemon of its pool — each with Resume and End.
//!
//! What lets any device get back to the one session: a phone that left it, a laptop opening the
//! page later, a second phone. Resume re-fits it to this device's screen and keeps its apps.

use dioxus::prelude::*;

use wado_protocol::SessionSummary;

use crate::{bridge, state::Ui, ui::widgets::Icon};

fn ago(secs: u64) -> String {
    match secs / 60 {
        0 => "just now".into(),
        m @ 1..=59 => format!("{m} min ago"),
        m => format!("{} h {} min ago", m / 60, m % 60),
    }
}

fn act(instance: &str, kind: &str) {
    bridge::call(format!(
        "window.__wado.sessionAct({}, {});",
        bridge::js(&instance),
        bridge::js(&kind)
    ));
}

/// The session "Join" takes you to: one nobody is watching first, then the newest.
pub fn best(list: &[SessionSummary]) -> Option<&SessionSummary> {
    list.iter().min_by_key(|s| (s.viewer.is_some(), s.age_s))
}

/// Join: take back the best running session, re-fitted to this screen.
pub fn join(ui: Ui) {
    let (_, list) = (ui.live.sessions)();
    if let Some(s) = best(&list) {
        act(&s.instance, "resume");
    }
}

/// New: start a session of its own. If the daemon this page is linked to already runs one, move
/// to an idle daemon of the pool first (the relay hands those out first).
pub fn new(ui: Ui) {
    let (here, list) = (ui.live.sessions)();
    if list.iter().any(|s| s.instance == here) {
        bridge::call("window.__wado.freshDaemon();".to_string());
    }
    crate::actions::start(ui);
}

fn short(app: &str) -> String {
    app.rsplit('.').next().unwrap_or(app).to_string()
}

pub fn render(ui: Ui) -> Element {
    let live = ui.live;
    // Asked while the home page is up — the one time the list is looked at.
    use_effect(move || {
        let home = !(live.session_on)();
        bridge::call(format!("window.__wado.sessionsWatch({home});"));
    });
    let (here, list) = (live.sessions)();
    let mut confirm = use_signal(|| None::<String>);
    if list.is_empty() {
        return rsx! {};
    }
    let title = if list.len() == 1 {
        "Active session".to_string()
    } else {
        format!("{} active sessions", list.len())
    };

    rsx! {
        div { class: "sect", span { "{title}" } }
        div { class: "sessions",
            for s in list.into_iter() {
                {
                    let (a, b, c) = (s.instance.clone(), s.instance.clone(), s.instance.clone());
                    let n = s.instance.rsplit(':').next().unwrap_or("").to_string();
                    let head = if s.apps.is_empty() { "Empty desktop".to_string() } else { short(&s.apps[0]) + if s.apps.len() > 1 { " and more" } else { "" } };
                    let who = match (&s.viewer, s.detached, s.instance == here) {
                        (Some(v), _, _) => format!("open on {v}"),
                        (None, true, _) => "left running".to_string(),
                        _ => "nobody watching".to_string(),
                    };
                    let live_now = s.viewer.is_some();
                    // The session's own shape, drawn: a phone-tall or a desk-wide box.
                    let (w, h) = (s.width.max(1) as f32, s.height.max(1) as f32);
                    let (bw, bh) = if w >= h { (46.0, 46.0 * h / w) } else { (46.0 * w / h, 46.0) };
                    let shape = format!("width:{bw:.0}px;height:{bh:.0}px");
                    let apps: Vec<String> = s.apps.iter().take(6).map(|a| short(a)).collect();
                    rsx! {
                        div { key: "{s.instance}", class: if live_now { "sessioncard watched" } else { "sessioncard" },
                            div { class: "sessionhead",
                                div { class: "sessionshape", title: "{s.width}×{s.height}",
                                    span { style: "{shape}" }
                                }
                                div { class: "navtext",
                                    b { "{head}" }
                                    small { "{s.width}×{s.height} · {s.fps} fps · started {ago(s.age_s)}" }
                                    small { class: "sessionwho", span { class: if live_now { "cdot on" } else { "cdot" } } "{who} · #{n}" }
                                }
                            }
                            if !apps.is_empty() {
                                div { class: "sessionapps",
                                    for (i, app) in apps.into_iter().enumerate() {
                                        span { key: "{i}{app}", class: "appchip", "{app}" }
                                    }
                                }
                            }
                            if confirm() == Some(c.clone()) {
                                div { class: "confirm",
                                    b { "End this session?" }
                                    p { class: "why", "Its apps are closed. Unsaved work in them is lost." }
                                    div { class: "actions",
                                        button { class: "btn", onclick: move |_| confirm.set(None), "Keep it" }
                                        button { class: "btn danger", onclick: move |_| { act(&b, "stop"); confirm.set(None); }, "End it" }
                                    }
                                }
                            } else {
                                div { class: "actions",
                                    button { class: "btn", onclick: move |_| confirm.set(Some(c.clone())), Icon { name: "power" } "End" }
                                    button { class: "btn primary", onclick: move |_| act(&a, "resume"), Icon { name: "play" } "Rejoin" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(i: &str, age: u64, viewer: bool) -> SessionSummary {
        SessionSummary {
            instance: i.into(),
            age_s: age,
            viewer: viewer.then(|| "Pixel".into()),
            ..Default::default()
        }
    }

    #[test]
    fn join_picks_an_unwatched_session_then_the_newest() {
        let l = [s("a", 10, true), s("b", 900, false), s("c", 60, false)];
        assert_eq!(best(&l).unwrap().instance, "c");
        assert_eq!(best(&[s("a", 10, true)]).unwrap().instance, "a");
    }
}
