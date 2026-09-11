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

        // Range and default are both far below what they were (0.2-5, default 1.0). Raw
        // browser deltas are already large — a wheel notch is ~100 px and a finger drag is
        // reported in CSS pixels — so a 1.0 multiplier meant "pass it straight through",
        // which overshoots on every device the client actually runs on. The old floor of
        // 0.2 was still too fast, so the floor is what moved most. Acceleration
        // (input_accel.js) is what makes a number this small still able to cross a page.
        label { "Scroll speed: {(s.scroll_speed)():.2}×" }
        input {
            r#type: "range", min: "0.05", max: "2", step: "0.05",
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
