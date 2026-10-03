//! The computer cards: pick one, name one, add one, forget one — each with its live status,
//! and the selected one's details once it has answered.
//!
//! Status starts with the page: the selected computer is dialled at load (`js/relay_link.js`)
//! and its card follows that link; the others are asked about at their relay (`js/online.js`).

use dioxus::prelude::*;

use crate::{
    profile,
    state::Ui,
    ui::{fetch::Fetch, pages::Page, widgets::Icon},
};

/// `(class, words)` for a card's status pill.
fn status(ui: Ui, selected: bool, key: &str) -> (&'static str, String) {
    let live = ui.live;
    if selected {
        let stage = (live.conn_stage)();
        let err = (live.conn_error)();
        return match () {
            _ if (live.session_on)() && stage >= 4 => ("ok", "Streaming".into()),
            _ if (live.hoststate)().is_some() || stage >= 2 => ("ok", "Online".into()),
            _ if err.contains("in use") => ("warn", "In use on another device".into()),
            _ if !err.is_empty() => ("bad", "Unreachable".into()),
            _ if stage == 1 => ("warn", "Computer offline".into()),
            _ => ("wait", "Connecting…".into()),
        };
    }
    let known = live.online.read().get(key).copied();
    match known {
        Some(n) if n > 0 => ("ok", "Online".into()),
        Some(0) => ("warn", "Computer offline".into()),
        Some(_) => ("bad", "Relay unreachable".into()),
        None => ("wait", "Checking…".into()),
    }
}

pub fn render(ui: Ui) -> Element {
    let mut s = ui.set;
    let mut live = ui.live;
    let list = s.profiles.read().clone();
    let cur = (s.profile)();
    let info = (live.hoststate)()
        .map(|h| h.info)
        .filter(|i| !i.hostname.is_empty());

    // Probe the computers this page is not dialled to; re-armed whenever the list changes.
    // Never during a session: its bandwidth belongs to the picture.
    use_effect(move || {
        let streaming = (live.session_on)();
        let list = if streaming {
            Vec::new()
        } else {
            s.profiles.read().clone()
        };
        let cur = (s.profile)();
        let targets: Vec<(String, String)> = list
            .iter()
            .enumerate()
            .filter(|(i, p)| *i != cur && p.conn_mode == "relay" && !p.remote_id.is_empty())
            .map(|(_, p)| (p.relay_url.clone(), p.remote_id.clone()))
            .collect();
        crate::bridge::call(format!(
            "window.__wado.onlineWatch({});",
            crate::bridge::js(&targets)
        ));
    });

    rsx! {
        div { class: "devices",
            if list.is_empty() {
                // No profile yet: the settings themselves are the one computer. It is filed as a
                // profile as soon as it says its hostname.
                {
                    let (tone, words) = status(ui, true, "");
                    rsx! {
                        div { class: "dev on",
                            button { class: "devpick",
                                onclick: move |_| { live.cc_page.set(Page::Connection); live.cc_open.set(true); },
                                span { class: "disc", style: "--hue:var(--base0D)", Icon { name: "server" } }
                                span { class: "navtext",
                                    b { "{profile::default_name(ui)}" }
                                    small { "{profile::capture(ui, String::new()).address()}" }
                                    span { class: "devstatus {tone}", "{words}" }
                                }
                            }
                        }
                    }
                }
            }
            for (i, p) in list.into_iter().enumerate() {
                {
                    let selected = i == cur;
                    let (tone, words) = status(ui, selected, &p.key());
                    rsx! {
                        div { key: "{i}", class: if selected { "dev on" } else { "dev" },
                            button { class: "devpick", onclick: move |_| profile::select(ui, i),
                                span { class: "disc", style: "--hue:var(--base{hue(i)})", Icon { name: "server" } }
                                span { class: "navtext",
                                    b { "{p.display()}" }
                                    small { "{p.address()}{shape(&p)}" }
                                    span { class: "devstatus {tone}", "{words}" }
                                }
                            }
                            button { class: "iconbtn", "aria-label": "Rename or edit {p.display()}",
                                onclick: move |_| { profile::select(ui, i); live.cc_page.set(Page::Connection); live.cc_open.set(true); },
                                Icon { name: "edit" }
                            }
                        }
                    }
                }
            }
            if let Some(info) = info.clone() {
                Fetch { info }
            }
            button { class: "dev add",
                onclick: move |_| {
                    // Before the first add, the computer in use becomes profile 0 so it is not lost.
                    if s.profiles.read().is_empty() {
                        let first = profile::capture(ui, String::new());
                        s.profiles.write().push(first);
                    }
                    let mut fresh = profile::capture(ui, String::new());
                    fresh.remote_id = String::new();
                    fresh.res = String::new();
                    s.profiles.write().push(fresh);
                    let i = s.profiles.read().len() - 1;
                    profile::select(ui, i);
                    live.cc_page.set(Page::Connection);
                    live.cc_open.set(true);
                },
                Icon { name: "plus" } "Add a computer"
            }
        }
    }
}

/// A different disc colour per card, so two computers are told apart at a glance.
fn hue(i: usize) -> &'static str {
    ["0D", "0E", "0C", "0B", "0A", "09"][i % 6]
}

fn shape(p: &profile::Profile) -> String {
    if p.res.is_empty() {
        String::new()
    } else {
        format!(" · {}@{}", p.res.replace('x', "×"), p.fps)
    }
}
