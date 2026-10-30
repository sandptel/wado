//! One running session on the Computer page's Sessions section: its shape, its apps, who is in it
//! (a chip per device, the lead marked), and what this device may do with it — Rejoin, Join, Take
//! over, End — each saying what it will do to the others before it does it.
//!
//! What is offered is [`crate::multi::offer`]'s rule; what each does is `js/multi.js`.

use dioxus::prelude::*;

use wado_protocol::SessionSummary;

use crate::{
    bridge, cfg,
    multi::Offer,
    state::Ui,
    ui::{home::sessions, widgets::Icon},
};

fn multi_act(ui: Ui, instance: &str, kind: &str) {
    let config = cfg::build(ui);
    bridge::call(format!(
        "window.__wado.multiAct({}, {}, {});",
        bridge::js(&instance),
        bridge::js(&kind),
        bridge::js(&config)
    ));
}

fn resume(instance: &str) {
    bridge::call(format!(
        "window.__wado.sessionAct({}, \"resume\");",
        bridge::js(&instance)
    ));
}

/// A device's colour (its participant hue, 0..8) as a base16 slot — the same as its dot over the
/// picture (`js/presence.js`).
pub fn hue_var(h: u8) -> String {
    let slot = b"89ABCDEF"[(h % 8) as usize] as char;
    format!("var(--base0{slot})")
}

/// "Pixel and Laptop", "Pixel, Laptop and 2 more".
fn names(list: &[String]) -> String {
    match list {
        [] => "nobody".into(),
        [a] => a.clone(),
        [a, b] => format!("{a} and {b}"),
        [a, b, rest @ ..] => format!("{a}, {b} and {} more", rest.len()),
    }
}

#[component]
pub fn SessionRow(
    ui: Ui,
    s: SessionSummary,
    offer: Offer,
    /// Its place in the list, for the stagger.
    i: usize,
    /// This device's id in it, when it is in it.
    you: u16,
    /// What is under way on it right now: `join`, `takeover`, `end`, or empty.
    busy: String,
) -> Element {
    let mut confirm = use_signal(|| None::<&'static str>);
    let head = sessions::headline(&s.apps);
    let shape = sessions::shape_style(s.width, s.height);
    let n = s.instance.rsplit(':').next().unwrap_or("").to_string();
    let apps: Vec<String> = s.apps.iter().take(5).map(|a| sessions::short(a)).collect();
    let others: Vec<String> = s
        .participants
        .iter()
        .filter(|p| p.id != you)
        .map(|p| p.name.clone())
        .collect();
    let count = s.participants.len();
    let state_word = match (count, s.detached, you != 0) {
        (_, _, true) => "you're in".to_string(),
        (0, true, _) => "left running".to_string(),
        (0, _, _) => "nobody in it".to_string(),
        (c, _, _) if s.max_participants > 0 => format!("{c} of {} in", s.max_participants),
        (c, _, _) => format!("{c} in"),
    };
    let live = count > 0 || s.viewer.is_some();
    let doing = match busy.as_str() {
        "join" => "Joining — testing the link…",
        "takeover" => "Taking over…",
        "end" => "Ending…",
        _ => "",
    };
    let inst = s.instance.clone();

    rsx! {
        article {
            class: if live { "srow live" } else { "srow" },
            style: "--i:{i}",
            "aria-label": "Session: {head}",
            div { class: "srowhead",
                div { class: "sessionshape", title: "{s.width}×{s.height}", span { style: "{shape}" } }
                div { class: "srowtext",
                    b { "{head}" }
                    small { class: "mono", "{s.width}×{s.height} · {s.fps} fps · #{n}" }
                }
                span { class: if live { "spill on" } else { "spill" }, i {} "{state_word}" }
            }
            if !s.participants.is_empty() {
                ul { class: "devchips", "aria-label": "Devices in it",
                    for (k, p) in s.participants.iter().enumerate() {
                        li {
                            key: "{p.id}",
                            class: if p.lead { "devchip lead" } else { "devchip" },
                            style: "--hue:{hue_var(p.hue)};--k:{k}",
                            title: if p.link.is_empty() { p.name.clone() } else { format!("{} · {}", p.name, p.link) },
                            i { class: "hdot" }
                            span { "{p.name}" }
                            if p.id == you && you != 0 { em { "you" } }
                            if p.lead { b { class: "leadmark", "lead" } }
                        }
                    }
                }
            } else if let Some(v) = s.viewer.clone() {
                ul { class: "devchips",
                    li { class: "devchip lead", style: "--hue:var(--accent)", title: "{v}", i { class: "hdot" } span { "{v}" } }
                }
            }
            if !apps.is_empty() {
                div { class: "sessionapps",
                    for (k, app) in apps.into_iter().enumerate() {
                        span { key: "{k}{app}", class: "appchip", "{app}" }
                    }
                }
            }
            if !offer.why_not.is_empty() && !offer.rejoin {
                p { class: "swhy", Icon { name: "info" } "Join: {offer.why_not}" }
            }
            if !doing.is_empty() {
                div { class: "sdoing", role: "status", i {} "{doing}" }
            } else if let Some(kind) = confirm() {
                div { class: "confirm sconfirm",
                    b {
                        if kind == "end" { "End it for everyone?" } else { "Take this session over?" }
                    }
                    p { class: "why",
                        if others.is_empty() {
                            if kind == "end" { "Its apps are closed. Unsaved work in them is lost." } else { "It is rebuilt at this screen's size." }
                        } else if kind == "end" {
                            "{names(&others)} will be disconnected, and its apps closed. Unsaved work in them is lost."
                        } else {
                            "{names(&others)} will be disconnected — they can rejoin. It is rebuilt at this screen's size."
                        }
                    }
                    div { class: "actions",
                        button { class: "btn", onclick: move |_| confirm.set(None), "Keep it" }
                        button {
                            class: if kind == "end" { "btn danger" } else { "btn primary" },
                            onclick: {
                                let inst = inst.clone();
                                move |_| {
                                    confirm.set(None);
                                    if kind == "end" {
                                        multi_act(ui, &inst, "end");
                                    } else {
                                        multi_act(ui, &inst, "takeover");
                                    }
                                }
                            },
                            if kind == "end" { "End it" } else { "Take over" }
                        }
                    }
                }
            } else {
                div { class: "sacts",
                    if offer.end {
                        button { class: "btn danger", onclick: move |_| confirm.set(Some("end")), Icon { name: "power" } "End" }
                    }
                    if offer.takeover {
                        button { class: "btn", onclick: move |_| confirm.set(Some("takeover")), Icon { name: "hand" } "Take over" }
                    }
                    if offer.join {
                        button {
                            class: "btn primary",
                            onclick: {
                                let inst = inst.clone();
                                move |_| multi_act(ui, &inst, "join")
                            },
                            Icon { name: "plus" }
                            "Join"
                        }
                    }
                    if offer.rejoin {
                        button {
                            class: "btn primary",
                            onclick: {
                                let inst = inst.clone();
                                move |_| resume(&inst)
                            },
                            Icon { name: "play", fill: true }
                            "Rejoin"
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn names_read_as_a_sentence() {
        let v = |s: &[&str]| s.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        assert_eq!(super::names(&v(&["A"])), "A");
        assert_eq!(super::names(&v(&["A", "B"])), "A and B");
        assert_eq!(super::names(&v(&["A", "B", "C", "D"])), "A, B and 2 more");
    }
}
