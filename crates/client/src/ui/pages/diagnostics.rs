//! Diagnostics: the readouts drawn over the picture, and the log.

use dioxus::prelude::*;

use crate::{debug, state::Ui, ui::widgets::SwitchRow};

pub fn render(ui: Ui) -> Element {
    let mut s = ui.set;
    let mut live = ui.live;
    let master = (s.debug_master)();

    rsx! {
        div { class: "card",
            SwitchRow {
                title: "Readouts over the picture",
                sub: "Off also stops the work behind them",
                on: master,
                ontoggle: move |v| { s.debug_master.set(v); debug::apply(ui); },
            }
            for (i, item) in debug::ITEMS.iter().enumerate() {
                SwitchRow {
                    key: "{item.id}",
                    title: item.label,
                    sub: "",
                    on: s.debug.read().get(i).copied().unwrap_or(item.default),
                    disabled: !master,
                    ontoggle: move |v| {
                        if let Some(flag) = s.debug.write().get_mut(i) { *flag = v; }
                        debug::apply(ui);
                    },
                }
            }
        }
        div { class: "card",
            button {
                class: "btn",
                onclick: move |_| {
                    live.console_tab.set("logs".to_string());
                    live.console_open.set(true);
                    live.cc_open.set(false);
                },
                "Open the server log"
            }
        }
    }
}
