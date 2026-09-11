//! Debug group: rendered entirely from [`crate::debug::ITEMS`].
//!
//! There is no per-toggle code here on purpose. A new debug view is a row in the registry and
//! nothing in this file changes — which is the point, because the bug this replaced was a
//! toggle that had been wired to three of its four places.

use dioxus::prelude::*;

use crate::{debug, state::Ui};

pub fn render(ui: Ui) -> Element {
    let mut s = ui.set;
    let master = (s.debug_master)();

    rsx! {
        label { class: "check master",
            input {
                r#type: "checkbox", checked: master,
                onchange: move |e| {
                    s.debug_master.set(e.checked());
                    debug::apply(ui);
                },
            }
            " Debug views"
        }

        div { class: "debug-items", hidden: !master,
            for (i, item) in debug::ITEMS.iter().enumerate() {
                label { key: "{item.id}", class: "check",
                    input {
                        r#type: "checkbox",
                        checked: s.debug.read().get(i).copied().unwrap_or(item.default),
                        onchange: move |e| {
                            // write() is scoped so the borrow is released before apply()
                            // reads the same signal back.
                            if let Some(flag) = s.debug.write().get_mut(i) {
                                *flag = e.checked();
                            }
                            debug::apply(ui);
                        },
                    }
                    " {item.label}"
                }
            }
        }
    }
}
