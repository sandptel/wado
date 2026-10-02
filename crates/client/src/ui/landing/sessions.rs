//! Running sessions on this computer — on whichever daemon of its pool — each with Resume and End.
//!
//! What lets any device get back to the one session: a phone that left it, a laptop opening the
//! page later, a second phone. Resume re-fits it to this device's screen and keeps its apps.

use dioxus::prelude::*;

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

pub fn render(ui: Ui) -> Element {
    let live = ui.live;
    // Asked while the home page is up — the one time the list is looked at.
    use_effect(move || {
        let home = !(live.session_on)();
        bridge::call(format!("window.__wado.sessionsWatch({home});"));
    });
    let (here, list) = (live.sessions)();
    if list.is_empty() {
        return rsx! {};
    }
    let mut confirm = use_signal(|| None::<String>);

    rsx! {
        div { class: "sect", span { "Running on this computer" } }
        div { class: "sessions",
            for s in list.into_iter() {
                {
                    let (a, b, c) = (s.instance.clone(), s.instance.clone(), s.instance.clone());
                    let n = s.instance.rsplit(':').next().unwrap_or("").to_string();
                    let apps = if s.apps.is_empty() { "No apps open".to_string() } else { s.apps.iter().map(|a| a.rsplit('.').next().unwrap_or(a).to_string()).collect::<Vec<_>>().join(", ") };
                    let who = match (&s.viewer, s.detached, s.instance == here) {
                        (Some(v), _, _) => format!("open on {v}"),
                        (None, true, _) => "left running".to_string(),
                        _ => "nobody watching".to_string(),
                    };
                    rsx! {
                        div { key: "{s.instance}", class: "sessioncard",
                            div { class: "sessionhead",
                                span { class: "sessionbadge", "#{n}" }
                                div { class: "navtext",
                                    b { "{apps}" }
                                    small { "{s.width}×{s.height} · {s.fps} fps · started {ago(s.age_s)} · {who}" }
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
                                    button { class: "btn primary", onclick: move |_| act(&a, "resume"), Icon { name: "play" } "Resume here" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
