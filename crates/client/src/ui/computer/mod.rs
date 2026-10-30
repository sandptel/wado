//! One computer's page: everything about it, named after it.
//!
//! | file | job |
//! |---|---|
//! | [`hero`] | its name, what it runs, the Link / Computer / Sessions strip, the hop bar |
//! | [`actions`] | Desktop · Trackpad · Files · Shell |
//! | [`playing`] | Now playing |
//! | [`quick`] | volume, brightness, keep awake, Wi-Fi, Bluetooth |
//! | [`clips`] | its clipboard: the Clipboard surface and this phone's sharing switches |
//! | [`grant`] | the card that says which grant is missing and how to allow it |
//! | [`settings_list`] | the four pages about it |
//! | [`pages`] | those pages |
//! | [`hops`] | the four connection hops |
//!
//! It is reached from its card on home by a shared-element transition (`js/hero_flip.js`): the
//! card's surface, disc and name grow into the hero's. The page is laid out by Rust; the motion
//! is all JS, transform and opacity only, and never takes input away — the live page is under
//! the finger from the first frame.

pub mod actions;
pub mod clips;
pub mod grant;
pub mod hero;
pub mod hops;
pub mod pages;
pub mod playing;
pub mod quick;
pub mod settings_list;

use dioxus::prelude::*;

use crate::{
    bridge, reach,
    route::{self, View},
    state::Ui,
    ui::widgets::Icon,
};

pub fn render(ui: Ui) -> Element {
    let view = (ui.live.view)();
    let Some(pk) = view.computer().map(str::to_string) else {
        return rsx! {};
    };
    let session = (ui.live.session_on)();
    let under = matches!(view, View::Sub(..));
    let r = reach::of(ui, &pk);
    let (name, hue) = match route::index_of(ui, &pk) {
        Some(i) => (ui.set.profiles.read()[i].display(), crate::profile::hue(i)),
        None => (crate::profile::default_name(ui), "0D"),
    };
    let off = !r.online && r.tone == "bad";
    let class = match (session, under) {
        (true, _) => "screen gone",
        (_, true) => "screen under",
        _ => "screen",
    };
    let host = r.host.clone();
    let k2 = pk.clone();

    rsx! {
        section {
            id: "computer",
            class: "{class}",
            style: "--hue:var(--base{hue})",
            "aria-hidden": "{session || under}",
            "aria-label": "{name}",
            div { class: "cground" }
            header { class: "cbar",
                button {
                    class: "iconbtn",
                    "aria-label": "Back to home",
                    onclick: move |_| bridge::call(format!("window.__wado.heroClose({});", bridge::js(&k2))),
                    Icon { name: "left" }
                }
                span { class: "cbtitle", "{name}" }
            }
            div { class: "cpage",
                div { class: "cgrid",
                    div { class: "col",
                        hero::Hero { ui, pk: pk.clone(), r: r.clone() }
                        if off {
                            div { class: "offcard sx o2", style: "--n:1",
                                b { "{name} is offline" }
                                p { "It is turned off, asleep, or its wado is not running. This page fills in by itself when it is back — nothing to retry." }
                            }
                        }
                        actions::Actions { ui, r: r.clone() }
                        // Claude's cards come over the selected computer's own link.
                        if r.selected && r.may("shells") && !off {
                            section { class: "sx claude o2", style: "--n:2", id: "claude", "aria-label": "Claude",
                                div { class: "sect", span { "Claude" } }
                                {crate::ui::scoped(ui, "claude", crate::ui::claude::section)}
                            }
                        }
                        section { class: "sx settings o5", style: "--n:4", "aria-label": "This computer",
                            div { class: "sect", span { "This computer" } }
                            {settings_list::render(ui)}
                        }
                    }
                    div { class: "col",
                        if !off {
                            section { class: "sx o3", style: "--n:2", "aria-label": "Now playing",
                                div { class: "sect", span { "Now playing" } }
                                match host.clone() {
                                    Some(h) => rsx! { playing::Playing { ui, pk: pk.clone(), name: name.clone(), h, may: r.may("host") } },
                                    None => rsx! { div { class: "npempty", Icon { name: "music" } "Asking {name}…" } },
                                }
                            }
                            section { class: "sx o4", style: "--n:3", "aria-label": "Quick",
                                div { class: "sect", span { "Quick" } }
                                match host {
                                    Some(h) => rsx! { quick::Quick { ui, pk: pk.clone(), name: name.clone(), h, r: r.clone() } },
                                    None => rsx! { div { class: "npempty", Icon { name: "sliders" } "Its controls show once it answers." } },
                                }
                            }
                            section { id: "clipsec", class: "sx o6 clipsec", style: "--n:5", "aria-labelledby": "clipsec-h",
                                div { class: "sect", span { id: "clipsec-h", "Clipboard" } }
                                clips::Clips { ui, pk: pk.clone(), name: name.clone(), r: r.clone() }
                            }
                        }
                    }
                }
            }
        }
    }
}
