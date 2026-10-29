//! Home's widgets: the few controls wanted without opening a computer, each bound to one
//! computer, in the order this phone keeps them (`crate::widget`). At the very end of home.
//!
//! | file | job |
//! |---|---|
//! | [`shell`] | one widget's card: header, host chip, edit-mode row |
//! | [`media`], [`volume`], [`brightness`], [`clipboard`], [`trackpad`] | each kind's body |
//! | [`edit`] | edit mode's hint and the Add a widget tray |
//! | [`picker`] | the sheet that picks a widget's computer |
//! | [`links`] | which computers hold a light link, so every widget is live |
//!
//! Motion lives in JS: `widget_flip.js` keeps neighbours continuous through every change (fold,
//! reorder, add, remove), `widget_drag.js` is the drag, `widget_touch.js` the trackpad's ring.

pub mod brightness;
pub mod clipboard;
pub mod edit;
pub mod links;
pub mod media;
pub mod picker;
pub mod shell;
pub mod trackpad;
pub mod volume;

use dioxus::prelude::*;

use crate::{bridge, state::Ui};

pub fn render(ui: Ui) -> Element {
    links::watch(ui);
    let s = ui.set;
    let mut live = ui.live;
    if s.profiles.read().is_empty() {
        // Nothing to bind a widget to yet.
        return rsx! {};
    }
    let list = s.widgets.read().clone();
    let editing = (live.widget_edit)();
    let n = list.len();
    rsx! {
        div { class: "sect", id: "wsect",
            span { "Widgets" }
            if n > 0 || editing {
                button {
                    "aria-pressed": "{editing}",
                    onclick: move |_| {
                        bridge::call("try { navigator.vibrate && navigator.vibrate(6); } catch (_) {}".to_string());
                        live.widget_edit.set(!editing);
                    },
                    if editing { "Done" } else { "Edit" }
                }
            }
        }
        if n == 0 && !editing {
            button { class: "wempty", onclick: move |_| live.widget_edit.set(true),
                crate::ui::widgets::Icon { name: "plus" } "Add a widget"
            }
        }
        div {
            class: if editing { "wlist editing" } else { "wlist" },
            onmounted: move |_| bridge::call("window.__wado.widgetFlip();".to_string()),
            for (k, w) in list.into_iter().enumerate() {
                shell::Card { key: "{w.id}", ui, w, k, n, editing }
            }
        }
        if editing {
            {edit::render(ui)}
        }
        {crate::ui::scoped(ui, "picker", picker::render)}
    }
}
