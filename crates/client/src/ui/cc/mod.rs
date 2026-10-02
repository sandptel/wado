//! The control centre: one sheet for everything that is not the picture.
//!
//! It replaces the docked settings column and the ⋯ pop-grid both (Decision Log 2026-10-02).
//! It floats over the stage and never takes space from it — a bottom sheet on a phone, a card
//! on the right on a desktop, the same markup for both. The root page is the hero, the tiles,
//! the shells and the settings list; every other page is pushed over it.

pub mod apply_bar;
pub mod hero;
pub mod list;
pub mod sound;
pub mod tiles;

use dioxus::prelude::*;

use crate::{
    actions, bridge,
    state::Ui,
    ui::{
        pages::{self, Page},
        widgets::Icon,
    },
};

pub fn render(ui: Ui) -> Element {
    let mut live = ui.live;
    let open = (live.cc_open)();
    let page = (live.cc_page)();
    let on = (live.session_on)();
    let relay = (ui.set.conn_mode)() == "relay";
    let shells = (live.shells)();
    let clips = (live.clips)();
    let notes = (live.notes)();

    rsx! {
        div {
            class: if open { "ccscrim open" } else { "ccscrim" },
            onclick: move |_| live.cc_open.set(false),
        }
        section {
            id: "cc",
            class: if open { "open" } else { "" },
            "aria-label": "Control centre",
            "aria-hidden": "{!open}",
            div { class: "grab", onclick: move |_| live.cc_open.set(false) }
            div { class: "pages",
                // The root stays mounted under a pushed page so coming back is instant and keeps
                // its scroll; the pushed page is mounted only while it is up.
                div { class: if page == Page::Root { "page" } else { "page behind" },
                    {hero::render(ui)}
                    {crate::ui::media::render(ui)}
                    if !notes.is_empty() {
                        div { class: "sect",
                            span { "Notifications" }
                            button { onclick: move |_| live.notes.write().clear(), "Clear all" }
                        }
                        div { class: "notes",
                            for n in notes.iter().cloned() {
                                div { key: "{n.id}", class: "note",
                                    span { class: "disc", style: "--hue:var(--base0A)", Icon { name: "bell" } }
                                    div { class: "notetext",
                                        small { "{n.app}" }
                                        b { "{n.summary}" }
                                        if !n.body.is_empty() { p { "{n.body}" } }
                                    }
                                    button { class: "iconbtn", "aria-label": "Dismiss",
                                        onclick: move |_| live.notes.write().retain(|x| x.id != n.id),
                                        Icon { name: "x" }
                                    }
                                }
                            }
                        }
                    }
                    {tiles::render(ui)}
                    {sound::render(ui)}
                    div { class: "sect",
                        span { "Shells" }
                        button {
                            disabled: !relay,
                            onclick: move |_| { live.cc_open.set(false); crate::ui::console::open_shells(ui); },
                            if shells.is_empty() { "Open" } else { "Show" }
                        }
                    }
                    if !shells.is_empty() {
                        div { class: "shellstrip",
                            for sh in shells.iter().cloned() {
                                button {
                                    key: "{sh.id}",
                                    class: if sh.alive { "shellcard" } else { "shellcard dead" },
                                    onclick: move |_| {
                                        live.cc_open.set(false);
                                        live.console_tab.set("shell".to_string());
                                        live.console_open.set(true);
                                        live.shell_active.set(sh.id);
                                        bridge::call(format!("window.__wado.shellShow({});", sh.id));
                                    },
                                    span { class: "disc", style: if sh.host.is_some() { "--hue:var(--base0E)" } else { "--hue:var(--base0B)" },
                                        Icon { name: if sh.host.is_some() { "server" } else { "term" } }
                                    }
                                    b { "{sh.title}" }
                                    small { if sh.host.is_some() { "ssh" } else { "this computer" } if !sh.alive { " · exited" } }
                                }
                            }
                        }
                    }
                    if !clips.is_empty() {
                        div { class: "sect", span { "Copied in the session" } }
                        div { class: "clips",
                            for (i, c) in clips.iter().cloned().enumerate() {
                                div { key: "{i}", class: "clip",
                                    span { class: "cliptext", "{c}" }
                                    button {
                                        class: "btn",
                                        onclick: move |_| bridge::call(format!("window.__wado.clipboardToPhone({});", bridge::js(&c))),
                                        "Copy"
                                    }
                                }
                            }
                        }
                    }
                    div { class: "sect", span { "Settings" }
                        button { onclick: move |_| live.cc_page.set(Page::Tiles), "Edit tiles" }
                    }
                    {list::render(ui)}
                    div { class: "actions",
                        button {
                            class: "btn", disabled: !on,
                            onclick: move |_| bridge::call("window.__wado.resync();".to_string()),
                            Icon { name: "refresh" } "Resync"
                        }
                        button {
                            class: "btn danger", disabled: !on,
                            onclick: move |_| { live.cc_open.set(false); actions::stop(ui); },
                            Icon { name: "power" } "End session"
                        }
                    }
                }
                if page != Page::Root {
                    div { key: "{page:?}", class: "page pushed",
                        div { class: "phead",
                            button { "aria-label": "Back", onclick: move |_| live.cc_page.set(Page::Root), Icon { name: "left" } }
                            h3 { "{page.title()}" }
                        }
                        {pages::render(ui, page)}
                    }
                }
            }
            {apply_bar::render(ui)}
        }
    }
}
