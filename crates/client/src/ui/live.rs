//! Live group: settings that take effect the moment they change, mid-session.
//!
//! Two kinds live here and they behave the same way from the user's side: things the browser
//! applies itself (move-mode, scroll) and things sent to the running session (launch, in
//! [`super::launcher`]). The distinction that mattered for grouping is "does this need a
//! restart", and none of these do.

use dioxus::prelude::*;

use crate::{bridge, state::Ui};

pub fn render(ui: Ui) -> Element {
    let mut s = ui.set;

    rsx! {
        {super::launcher::render(ui)}

        label {
            class: "check",
            input {
                r#type: "checkbox", checked: (s.move_mode)(),
                onchange: move |e| {
                    let c = e.checked();
                    s.move_mode.set(c);
                    bridge::call(format!("window.__wado.setMoveMode({c});"));
                },
            }
            " Move-window mode (drag moves windows)"
        }

        label { "Scroll speed: {(s.scroll_speed)():.1}×" }
        input {
            r#type: "range", min: "0.2", max: "5", step: "0.1",
            value: "{(s.scroll_speed)()}",
            oninput: move |e| {
                if let Ok(v) = e.value().parse::<f64>() {
                    s.scroll_speed.set(v);
                    bridge::call(format!(
                        "window.__wado.setScroll({v}, {});", (s.natural_scroll)()
                    ));
                }
            },
        }
        label {
            class: "check",
            input {
                r#type: "checkbox", checked: (s.natural_scroll)(),
                onchange: move |e| {
                    let c = e.checked();
                    s.natural_scroll.set(c);
                    bridge::call(format!(
                        "window.__wado.setScroll({}, {c});", (s.scroll_speed)()
                    ));
                },
            }
            " Natural scroll direction"
        }
    }
}
