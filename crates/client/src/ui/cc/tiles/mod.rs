//! The quick-tile grid. Tap toggles; long-press (or right-click) opens the tile's page.
//!
//! Long-press is `oncontextmenu`, which a touch long-press and a desktop right-click both raise
//! — no timers, the same trick the app drawer uses.

pub mod pool;

use dioxus::prelude::*;

use crate::{state::Ui, ui::widgets::Icon};
use pool::Act;

pub fn render(ui: Ui) -> Element {
    let ids = ui.set.tiles.read().clone();
    let on_session = (ui.live.session_on)();
    let mut live = ui.live;

    rsx! {
        div { class: "tiles",
            for id in ids {
                if let Some(t) = pool::get(&id) {
                    {
                        let on = t.on.is_some_and(|f| f(ui));
                        let off = t.needs_session && !on_session;
                        let class = match (on, off) {
                            (_, true) => "tile off",
                            (true, _) => "tile on",
                            _ => "tile",
                        };
                        let sub = if on { t.sub.1 } else { t.sub.0 };
                        let page = t.page;
                        let body = rsx! {
                            span { class: "ic", Icon { name: t.icon } }
                            span { class: "tl", b { "{t.label}" } small { "{sub}" } }
                        };
                        let menu = move |e: Event<MouseData>| {
                            if let Some(p) = page {
                                e.prevent_default();
                                live.cc_page.set(p);
                            }
                        };
                        match t.act {
                            Act::Keyboard => rsx! {
                                label { key: "{t.id}", r#for: "wado-osk", class, oncontextmenu: menu, {body} }
                            },
                            Act::PointerLock => rsx! {
                                button { key: "{t.id}", id: "wado-lock", class, disabled: off, "aria-pressed": "{on}", oncontextmenu: menu, {body} }
                            },
                            Act::Run(f) => rsx! {
                                button { key: "{t.id}", class, disabled: off, "aria-pressed": "{on}",
                                    onclick: move |_| f(ui), oncontextmenu: menu, {body}
                                }
                            },
                        }
                    }
                }
            }
        }
    }
}
