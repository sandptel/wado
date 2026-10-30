//! The Computer page's Virtual input tile (D4): a group, not a button. Its heading opens the hub;
//! three 44 px doors open Touchpad, Keyboard or Gamepad in one tap — the frequent job is "use
//! it", not "configure it". A shut door says why in its label, as the old Trackpad tile did.

use dioxus::prelude::*;

use super::open::{self, Door};
use crate::{reach::Reach, state::Ui, ui::widgets::Icon};

#[component]
pub fn Tile(ui: Ui, r: Reach) -> Element {
    let doors = [
        (Door::Touchpad, "Touchpad", "trackpad"),
        (Door::Keyboard, "Keyboard", "kbd"),
        (Door::Gamepad, "Gamepad", "pad"),
    ];
    let any_shut = doors
        .iter()
        .all(|(d, _, _)| open::shut(ui, &r, *d).is_some());
    let sub = match open::shut(ui, &r, Door::Touchpad) {
        Some(why) if any_shut => why,
        _ => "This phone as its…".to_string(),
    };
    rsx! {
        div { class: "act vi", role: "group", "aria-label": "Virtual input",
            button { class: "vihead", onclick: move |_| open::hub(ui),
                span { class: "aic", Icon { name: "phone" } }
                span { b { "Virtual input" } small { "{sub}" } }
            }
            div { class: "vidoors",
                for (door, name, icon) in doors {
                    {
                        let why = open::shut(ui, &r, door);
                        let label = match &why { Some(w) => format!("{name} — {w}"), None => name.to_string() };
                        rsx! {
                            button {
                                key: "{name}",
                                class: "vidoor",
                                "aria-label": "{label}",
                                title: "{label}",
                                disabled: why.is_some(),
                                onclick: move |_| open::open(ui, door),
                                Icon { name: icon }
                                span { class: "vidoorname", "{name}" }
                            }
                        }
                    }
                }
            }
        }
    }
}
