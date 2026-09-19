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
            // The app drawer. Beside the window actions because it is the same kind of thing:
            // something you do to the running session, and pointless without one.
            button {
                class: "barbtn",
                title: "Apps",
                "aria-label": "Apps",
                disabled: !(live.session_on)(),
                onclick: move |_| {
                    let open = !(live.drawer_open)();
                    live.drawer_open.set(open);
                    // Re-ask on every open. The list carries which applications are running,
                    // and that is only true at the moment it is answered — a dot from two
                    // minutes ago is worse than no dot. Cheap: the scan is a few milliseconds
                    // of directory reads and one round trip to the compositor.
                    if open {
                        bridge::call("window.__wado.requestApps();".to_string());
                    }
                },
                "⊞"
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

            // One button for the console — shell and log together. Two buttons for two
            // panels would be two rows of chrome over a phone screen that has none to give.
            // Beside the window actions because it is the same kind of thing: something you
            // do to the running session, and pointless without one.
            button {
                class: "barbtn",
                title: "Console (shell and log)",
                "aria-label": "Console",
                disabled: !(live.session_on)(),
                onclick: move |_| {
                    let open = !(live.console_open)();
                    live.console_open.set(open);
                    // Focus follows the reveal, but only for the shell: opening a command
                    // line and then having to tap it is a step too many on a phone. On the
                    // log tab a focused input would raise the keyboard over what you opened
                    // the panel to read.
                    // The terminal is hidden with CSS, so revealing it is what tells the
                    // emulator to re-measure — a terminal sized while display:none is 1x1.
                    // This is also what starts the shell the first time.
                    if open && (live.console_tab)() == "shell" {
                        bridge::call("window.__wado.ptyShow();".to_string());
                    }
                },
                "❯_"
            }

            // A phone has no keys. Focusing a hidden input is what raises the soft keyboard;
            // `zwp_text_input_v3` cannot do this job because smithay drops every text-input
            // request with no input-method client bound. See `js/osk.js`.
            //
            // A `label`, not a `button`, and that is the whole fix: Android raises the soft
            // keyboard only for a focus that happens *inside* the user gesture, and
            // `bridge::call` is `spawn(async { eval().await })` — by the time `focus()` ran the
            // gesture was over, so the old button could never work on a phone however correct
            // the rest of it was. Label activation focuses its `for` target natively, in the
            // gesture, with no JS in the path at all.
            label {
                r#for: "wado-osk",
                class: if !(live.session_on)() { "barbtn off" }
                       else if (live.osk_on)() { "barbtn active" }
                       else { "barbtn" },
                title: "Keyboard",
                "aria-label": "Keyboard",
                "⌨"
            }

            // Pointer lock, for games. No `onclick`: `js/input_lock.js` listens for the click
            // on this id in the capture phase, because `requestPointerLock` needs the user
            // gesture and a Dioxus handler's `bridge::call` is an async eval that arrives after
            // it. The state comes back from `pointerlockchange`, which is also what turns this
            // off when the browser drops the lock on Escape.
            button {
                id: "wado-lock",
                class: if (live.pointer_lock)() { "barbtn active" } else { "barbtn" },
                title: "Take the mouse (for games) — Escape releases it",
                "aria-label": "Take the mouse",
                "aria-pressed": "{(live.pointer_lock)()}",
                disabled: !(live.session_on)(),
                "🎯"
            }

            // A fresh peer connection, which is the only thing that resets the browser's
            // playout buffer after a network hitch has permanently inflated it. The compositor
            // session survives — windows, apps and the shell are not tied to the connection.
            button {
                class: "barbtn",
                title: "Resync video (rebuilds the connection, keeps the session)",
                "aria-label": "Resync video",
                disabled: !(live.session_on)(),
                onclick: move |_| bridge::call("window.__wado.resync();".to_string()),
                "⟳"
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
