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

/// The window actions, in the order they sit on the bar: the two that change a window's size
/// first, then the destructive one, then the one that moves on. `close` is deliberately not
/// adjacent to `cycle` — a mis-tap between "next window" and "close this window" is the one
/// mistake here that cannot be undone.
const WINDOW_ACTIONS: &[(&str, &str, &str)] = &[
    ("maximize", "❐", "Maximize / restore"),
    ("minimize", "—", "Send to back"),
    ("close", "✕", "Close window"),
    ("cycle_focus", "⇄", "Next window"),
];

pub fn render(ui: Ui) -> Element {
    let mut live = ui.live;
    let mut set = ui.set;
    let open = (live.sheet_open)();
    let panel = (set.panel_open)();

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
            // Desktop counterpart of the ⚙: below the breakpoint the panel is a sheet the
            // scrim already dismisses, so this is hidden there rather than duplicating it.
            button {
                class: "barbtn docked-only",
                title: if panel { "Hide settings panel" } else { "Show settings panel" },
                "aria-label": if panel { "Hide settings panel" } else { "Show settings panel" },
                onclick: move |_| set.panel_open.set(!panel),
                if panel { "⟨" } else { "⟩" }
            }
            // Window actions need a running session to act on; the bar itself does not.
            for (action, glyph, title) in WINDOW_ACTIONS {
                button {
                    key: "{action}",
                    class: "barbtn",
                    title: "{title}",
                    "aria-label": "{title}",
                    disabled: !(live.session_on)(),
                    onclick: move |_| bridge::call(format!(
                        "window.__wado.windowAction({});", bridge::js(action)
                    )),
                    "{glyph}"
                }
            }

            // Beside the window actions, because it belongs to the same group: things you do
            // to the running session. A session is required for the same reason they are —
            // there is no shell to run in until one exists.
            button {
                class: "barbtn",
                title: "Shell",
                "aria-label": "Shell",
                disabled: !(live.session_on)(),
                onclick: move |_| {
                    let open = !(live.term_open)();
                    live.term_open.set(open);
                    // Focus follows the reveal: opening a command line and then having to
                    // tap it is one step too many on a phone.
                    if open {
                        bridge::call(
                            "setTimeout(function(){var e=document.getElementById('wado-terminput'); \
                             if(e) e.focus();}, 60);"
                                .to_string(),
                        );
                    }
                },
                "❯_"
            }

            span { class: "barsep" }

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
