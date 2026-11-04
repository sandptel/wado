//! Home's top bar (the app's `ui/home/top_bar.dart`): the computer dropdown on the left — All
//! computers first — then `?` (what can I do here) and `⋯`, the picked computer's settings and
//! connection, named so it is never ambiguous which one it acts on. A native `<select>`-free menu:
//! a popover list that Esc and a click outside close, keyboard-walkable.

use dioxus::prelude::*;

use crate::{
    profile, route,
    state::Ui,
    ui::{pages::Page, widgets::Icon},
};

pub fn render(ui: Ui) -> Element {
    let mut open = use_signal(|| false);
    let mut more = use_signal(|| false);
    let mut help = use_signal(|| false);
    let list = ui.set.profiles.read().clone();
    let cur = (ui.set.profile)();
    let all = (ui.set.home_all)() && list.len() > 1;
    let name = if all { "All computers".to_string() } else { profile::current_name(ui) };
    let hue = if all { "0D".to_string() } else { profile::hue(cur).to_string() };
    rsx! {
        div { class: "topbar",
            div { class: "tbwrap",
                button {
                    class: "hostdd",
                    "aria-haspopup": "menu",
                    "aria-expanded": "{open()}",
                    onclick: move |_| { open.toggle(); more.set(false); },
                    i { style: "--hue: var(--base{hue})" }
                    span { "{name}" }
                    Icon { name: "down" }
                }
                if open() {
                    div { class: "tbscrim", onclick: move |_| open.set(false) }
                    div { class: "tbmenu", role: "menu", onkeydown: move |e: KeyboardEvent| if e.key() == Key::Escape { open.set(false) },
                        if list.len() > 1 {
                            button { class: if all { "tbitem on" } else { "tbitem" }, role: "menuitemradio", "aria-checked": "{all}",
                                onclick: move |_| { let mut a = ui.set.home_all; a.set(true); open.set(false); },
                                i { style: "--hue: var(--accent)" }
                                span { b { "All computers" } small { "One home made from every computer" } }
                            }
                        }
                        for (i, c) in list.iter().enumerate() {
                            button {
                                key: "{c.key()}",
                                class: if !all && i == cur { "tbitem on" } else { "tbitem" },
                                role: "menuitemradio",
                                "aria-checked": "{!all && i == cur}",
                                onclick: move |_| {
                                    let mut a = ui.set.home_all;
                                    a.set(false);
                                    if (ui.set.profile)() != i { profile::select(ui, i); }
                                    open.set(false);
                                },
                                i { style: "--hue: var(--base{profile::hue(i)})" }
                                span { b { "{c.display()}" } small { if c.os.is_empty() { "{c.address()}" } else { "{c.os}" } } }
                            }
                        }
                        button { class: "tbitem", role: "menuitem", onclick: move |_| { open.set(false); route::go(ui, Page::AddComputer); },
                            Icon { name: "plus" }
                            span { b { "Add a computer" } }
                        }
                    }
                }
            }
            span { class: "tbsp" }
            button { class: "iconbtn tbq", "aria-label": "What can I do here?", onclick: move |_| help.set(true), "?" }
            div { class: "tbwrap",
                button { class: "iconbtn", "aria-label": "{profile::current_name(ui)}: settings and connection", "aria-haspopup": "menu", "aria-expanded": "{more()}",
                    onclick: move |_| { more.toggle(); open.set(false); },
                    Icon { name: "dots" }
                }
                if more() {
                    div { class: "tbscrim", onclick: move |_| more.set(false) }
                    div { class: "tbmenu right", role: "menu", onkeydown: move |e: KeyboardEvent| if e.key() == Key::Escape { more.set(false) },
                        div { class: "tbhead", "{profile::current_name(ui)}" }
                        button { class: "tbitem", role: "menuitem", onclick: move |_| { more.set(false); route::open_computer(ui, &profile::current_key(ui)); },
                            Icon { name: "monitor" }
                            span { b { "Settings & connection" } }
                        }
                        button { class: "tbitem", role: "menuitem", onclick: move |_| { more.set(false); route::go(ui, Page::Devices); },
                            Icon { name: "phone" }
                            span { b { "Devices" } small { "Who is signed in" } }
                        }
                    }
                }
            }
        }
        if help() {
            {crate::ui::surround::help::overlay(ui, "home", move || help.set(false))}
        }
    }
}
