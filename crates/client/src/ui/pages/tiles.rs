//! Edit tiles: order, remove, and add from the pool.
//!
//! ponytail: arrows rather than drag-to-reorder. Drag is the upgrade once the tile count makes
//! arrow-tapping tedious; at a dozen it is not.

use dioxus::prelude::*;

use crate::{
    state::Ui,
    ui::{cc::tiles::pool, widgets::Icon},
};

pub fn render(ui: Ui) -> Element {
    let mut s = ui.set;
    let list = s.tiles.read().clone();
    let spare: Vec<_> = pool::ALL
        .iter()
        .filter(|t| !list.iter().any(|id| id == t.id))
        .collect();
    let n = list.len();

    rsx! {
        div { class: "card",
            div { class: "cardhead", "Shown" }
            for (i, id) in list.iter().cloned().enumerate() {
                if let Some(t) = pool::get(&id) {
                    div { key: "{id}", class: "editrow",
                        span { class: "disc", Icon { name: t.icon } }
                        b { "{t.label}" }
                        button { "aria-label": "Move up", disabled: i == 0,
                            onclick: move |_| s.tiles.write().swap(i, i - 1), Icon { name: "up" } }
                        button { "aria-label": "Move down", disabled: i + 1 == n,
                            onclick: move |_| s.tiles.write().swap(i, i + 1), Icon { name: "down" } }
                        button { "aria-label": "Remove",
                            onclick: move |_| { s.tiles.write().remove(i); }, Icon { name: "x" } }
                    }
                }
            }
        }
        if !spare.is_empty() {
            div { class: "card",
                div { class: "cardhead", "More tiles" }
                for t in spare {
                    div { key: "{t.id}", class: "editrow",
                        span { class: "disc", Icon { name: t.icon } }
                        b { "{t.label}" }
                        button { "aria-label": "Add", onclick: move |_| s.tiles.write().push(t.id.to_string()), Icon { name: "plus" } }
                    }
                }
            }
        }
        button { class: "btn", onclick: move |_| s.tiles.set(pool::defaults()), "Reset to the default eight" }
    }
}
