//! Live group: settings that take effect the moment they change, mid-session.
//!
//! Two kinds live here and they behave the same way from the user's side: things the browser
//! applies itself (move-mode, scroll) and things sent to the running session (launch, in
//! [`super::launcher`]). The distinction that mattered for grouping is "does this need a
//! restart", and none of these do.

use dioxus::prelude::*;

use crate::{bridge, state::Ui};

/// Push the JS-backed Live settings to the bridge.
///
/// The counterpart to [`crate::debug::apply`], and needed for the same reason: persistence
/// restores the *signals*, but the browser-side state these mirror only ever changed inside
/// the handlers below. Without this call, a reload shows "natural scroll, 3.0x" in the panel
/// while the bridge is still scrolling at 1.0x in the other direction.
///
/// It is also the single place that knows the call shape, so the handlers cannot drift from
/// each other — `setScroll` takes both values, and updating either one has to send both.
pub fn apply(ui: Ui) {
    let s = ui.set;
    bridge::call(format!("window.__wado.setMoveMode({});", (s.move_mode)()));
    bridge::call(format!(
        "window.__wado.setScroll({}, {});",
        (s.scroll_speed)(),
        (s.natural_scroll)()
    ));
}

pub fn render(ui: Ui) -> Element {
    let mut s = ui.set;

    rsx! {
        {super::launcher::render(ui)}

        label {
            class: "check",
            input {
                r#type: "checkbox", checked: (s.move_mode)(),
                onchange: move |e| {
                    s.move_mode.set(e.checked());
                    apply(ui);
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
                    apply(ui);
                }
            },
        }
        label {
            class: "check",
            input {
                r#type: "checkbox", checked: (s.natural_scroll)(),
                onchange: move |e| {
                    s.natural_scroll.set(e.checked());
                    apply(ui);
                },
            }
            " Natural scroll direction"
        }
    }
}
