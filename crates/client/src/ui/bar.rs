//! The control bar: actions that need to be reachable mid-session without opening settings.
//!
//! It floats over the video and fades when idle (see `js/bar.js`), because everything on it
//! replaces something a desktop does with a keyboard shortcut — and a shortcut that costs
//! open-panel → tap → dismiss is not a shortcut.
//!
//! On desktop the settings panel is always docked, so the ⚙ is hidden there by CSS rather
//! than by a branch here; the bar itself stays, since fullscreen is useful on both.

use dioxus::prelude::*;

use crate::{bridge, cfg, state::Ui};

pub fn render(ui: Ui) -> Element {
    let mut live = ui.live;
    let open = (live.sheet_open)();

    rsx! {
        // No `idle` class here: the bar starts visible and js/bar.js fades it on a timer.
        // Rendering it hidden would make it undiscoverable on first load.
        div { id: "wado-bar",
            button {
                class: "barbtn settings-only",
                title: "Settings",
                "aria-label": "Settings",
                onclick: move |_| live.sheet_open.set(!open),
                "⚙"
            }
            button {
                class: "barbtn",
                title: "Fullscreen",
                "aria-label": "Fullscreen",
                onclick: move |_| {
                    // The session's own dimensions, so the orientation lock matches the
                    // output rather than whichever way the phone happens to be held.
                    let c = cfg::build(ui);
                    bridge::call(format!(
                        "window.__wado.toggleFullscreen({}, {});", c.width, c.height
                    ));
                },
                "⛶"
            }
        }
    }
}
