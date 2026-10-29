//! The computer cards: one per saved computer, each with its live status, opening that
//! computer's page — and a ▶ that goes straight to its desktop, starting one or rejoining the
//! one running, with no page in between.
//!
//! Status starts with the page: the selected computer is dialled at load (`js/relay_link.js`),
//! the ones with a widget hold a light link (`js/side_link.js`), and the rest are asked about at
//! their relay (`js/online.js`). [`crate::reach`] reads whichever applies.

use dioxus::prelude::*;

use crate::{
    actions, profile, reach,
    route::{self, View},
    state::Ui,
    ui::{home::sessions, pages::Page, widgets::Icon},
};

/// Start or rejoin the desktop on computer `i`, selecting it first.
fn go(ui: Ui, i: usize) {
    if (ui.set.profile)() != i {
        profile::select(ui, i);
        actions::start(ui);
        return;
    }
    if (ui.live.sessions)().1.is_empty() {
        actions::start(ui);
    } else {
        sessions::join(ui);
    }
}

pub fn render(ui: Ui) -> Element {
    let s = ui.set;
    let live = ui.live;
    let list = s.profiles.read().clone();
    let cur = (s.profile)();
    let session_n = (live.sessions)().1.len();

    // Probe the computers this page is not dialled to and holds no light link to; re-armed
    // whenever the list changes. Never during a session: its bandwidth belongs to the picture.
    use_effect(move || {
        let streaming = (live.session_on)();
        let list = if streaming {
            Vec::new()
        } else {
            s.profiles.read().clone()
        };
        let cur = (s.profile)();
        let side: Vec<String> = live.side.read().keys().cloned().collect();
        let targets: Vec<(String, String)> = list
            .iter()
            .enumerate()
            .filter(|(i, p)| {
                *i != cur
                    && p.conn_mode == "relay"
                    && !p.remote_id.is_empty()
                    && !side.contains(&p.key())
            })
            .map(|(_, p)| (p.relay_url.clone(), p.remote_id.clone()))
            .collect();
        crate::bridge::call(format!(
            "window.__wado.onlineWatch({});",
            crate::bridge::js(&targets)
        ));
    });

    rsx! {
        div { class: "ccards",
            if list.is_empty() {
                // No profile yet: the settings themselves are the one computer. It is filed as a
                // profile as soon as it says its hostname.
                {
                    let r = reach::of(ui, "");
                    rsx! {
                        div { class: "ccard", "data-key": "",
                            button { class: "cpick", "aria-label": "{profile::default_name(ui)} — open its page",
                                onclick: move |_| route::open_computer(ui, ""),
                                span { class: "disc", style: "--hue:var(--base0D)", Icon { name: "monitor" } }
                                span { class: "ctext",
                                    b { class: "cname", "{profile::default_name(ui)}" }
                                    span { class: "cos", "{profile::capture(ui, String::new()).address()}" }
                                    span { class: "cline", span { class: "lstate {r.tone}", i { class: "ldot" } span { "{r.words}" } } }
                                }
                            }
                            button { class: "cgo", disabled: !r.online, "aria-label": "Start a desktop",
                                onclick: move |_| actions::start(ui),
                                Icon { name: "play", fill: true }
                            }
                        }
                    }
                }
            }
            for (i, p) in list.into_iter().enumerate() {
                {
                    let key = p.key();
                    let r = reach::of(ui, &key);
                    let n = if i == cur { session_n } else { 0 };
                    let os = [p.os.as_str(), p.desktop.as_str()].into_iter().filter(|x| !x.is_empty()).collect::<Vec<_>>().join(" · ");
                    let os = if os.is_empty() { p.address() } else { os };
                    let k1 = key.clone();
                    let name = p.display();
                    rsx! {
                        div { key: "{key}", class: "ccard", "data-key": "{key}", style: "--hue:var(--base{profile::hue(i)})",
                            button { class: "cpick", "aria-label": "{name} — open its page",
                                onclick: move |_| route::open_computer(ui, &k1),
                                span { class: "disc", Icon { name: profile::icon(&p) } }
                                span { class: "ctext",
                                    b { class: "cname", "{name}" }
                                    span { class: "cos", "{os}" }
                                    span { class: "cline",
                                        span { class: "lstate {r.tone}", i { class: "ldot" } span { "{r.words}" } }
                                        if n > 0 { span { if n == 1 { "· 1 session" } else { "· {n} sessions" } } }
                                    }
                                }
                            }
                            button {
                                class: if n > 0 { "cgo live" } else { "cgo" },
                                disabled: !r.online && !r.busy,
                                "aria-label": if n > 0 { "Rejoin the desktop on {name}" } else { "Start a desktop on {name}" },
                                onclick: move |_| go(ui, i),
                                Icon { name: "play", fill: true }
                            }
                        }
                    }
                }
            }
            button { class: "ccard add",
                onclick: move |_| {
                    let mut view = ui.live.view;
                    view.set(View::Sub(None, Page::AddComputer));
                },
                Icon { name: "plus" } "Add a computer"
            }
        }
    }
}
