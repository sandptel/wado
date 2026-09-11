//! Live group: settings that take effect the moment they change, mid-session.
//!
//! Two kinds live here and they behave the same way from the user's side: things the browser
//! applies itself (move-mode, scroll) and things sent to the running session (launch). The
//! distinction that mattered for grouping is "does this need a restart", and none of these do.

use dioxus::prelude::*;

use crate::{actions, bridge, state::Ui};

pub fn render(ui: Ui) -> Element {
    let mut s = ui.set;
    let on = (ui.live.session_on)();
    let can_launch = on && !(s.command)().trim().is_empty();

    rsx! {
        label { "Command" }
        p { class: "hint", "Spawn a command into the running session — as many as you like." }
        input {
            r#type: "text",
            value: "{(s.command)()}",
            placeholder: "e.g. weston-terminal",
            oninput: move |e| s.command.set(e.value()),
        }
        button {
            id: "launch", class: "wide",
            disabled: !can_launch,
            onclick: move |_| actions::launch(ui),
            "Launch into session"
        }

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
