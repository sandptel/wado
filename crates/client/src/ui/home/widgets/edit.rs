//! Edit mode's footer: how to arrange, and the Add a widget tray — every kind, any number of
//! times. A new widget follows the selected computer; its chip changes that.

use dioxus::prelude::*;

use crate::{
    state::Ui,
    ui::widgets::Icon,
    widget::{self, WidgetKind},
};

pub fn render(ui: Ui) -> Element {
    let mut s = ui.set;
    rsx! {
        p { class: "edithint", "Drag the dots to reorder, or use the arrows. Each widget follows one computer — tap its name to change it." }
        div { class: "wtray",
            div { class: "sect", span { "Add a widget" } }
            div { class: "traygrid",
                for kind in WidgetKind::ALL {
                    button {
                        key: "{kind.short()}",
                        class: "wadd",
                        onclick: move |_| {
                            let host = crate::profile::current_key(ui);
                            widget::add(&mut s.widgets.write(), kind, &host);
                        },
                        Icon { name: "plus" }
                        span { class: "wadd-ic", Icon { name: kind.icon() } }
                        "{kind.short()}"
                    }
                }
            }
        }
    }
}
