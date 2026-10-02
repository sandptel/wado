//! The host cards: pick one, add one, forget one.

use dioxus::prelude::*;

use crate::{
    profile,
    state::Ui,
    ui::{pages::Page, widgets::Icon},
};

pub fn render(ui: Ui) -> Element {
    let mut s = ui.set;
    let mut live = ui.live;
    let list = s.profiles.read().clone();
    let cur = (s.profile)();

    rsx! {
        div { class: "devices",
            if list.is_empty() {
                // No profile yet: the settings themselves are the one host. Start saves it.
                button { class: "dev on",
                    onclick: move |_| { live.cc_page.set(Page::Connection); live.cc_open.set(true); },
                    span { class: "disc", style: "--hue:var(--base0D)", Icon { name: "server" } }
                    span { class: "navtext", b { "{profile::default_name(ui)}" } small { "{profile::capture(ui, String::new()).address()}" } }
                    Icon { name: "edit" }
                }
            }
            for (i, p) in list.into_iter().enumerate() {
                div { key: "{i}", class: if i == cur { "dev on" } else { "dev" },
                    button { class: "devpick", onclick: move |_| profile::select(ui, i),
                        span { class: "disc", style: "--hue:var(--base{hue(i)})", Icon { name: "server" } }
                        span { class: "navtext",
                            b { "{p.name}" }
                            small {
                                "{p.address()}{shape(&p)}"
                            }
                        }
                    }
                    button { class: "iconbtn", "aria-label": "Edit {p.name}",
                        onclick: move |_| { profile::select(ui, i); live.cc_page.set(Page::Connection); live.cc_open.set(true); },
                        Icon { name: "edit" }
                    }
                    button { class: "iconbtn", "aria-label": "Forget {p.name}",
                        onclick: move |_| {
                            s.profiles.write().remove(i);
                            let n = s.profiles.read().len();
                            if cur >= n { s.profile.set(n.saturating_sub(1)); }
                        },
                        Icon { name: "x" }
                    }
                }
            }
            button { class: "dev add",
                onclick: move |_| {
                    let name = format!("Computer {}", s.profiles.read().len() + 1);
                    // Before the first add, the settings in use become profile 0 so they are not lost.
                    if s.profiles.read().is_empty() {
                        let first = profile::capture(ui, profile::default_name(ui));
                        s.profiles.write().push(first);
                    }
                    let mut fresh = profile::capture(ui, name);
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

/// A different disc colour per card, so two hosts are told apart at a glance.
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
