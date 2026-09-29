//! The control bar: three buttons, Android-style — **⋯ More · ○ Apps · ◁ Back**, left to
//! right, so Back sits under a right thumb and the least reachable slot holds the adaptive one.
//!
//! Back is what gets pressed constantly (see `WindowAction::Back`); Apps is "home"; switching
//! apps is the dial's job. Everything else is occasional and lives in More's sheet. More adapts
//! to the one moment it is worth a slot: an app wants typing and the keyboard is down, so it
//! becomes ⌨. Decided 2026-09-29 with the user; see the Decision Log.
//!
//! It floats over the video and fades when idle (see `js/bar.js`).

use dioxus::prelude::*;

use crate::{bridge, cfg, state::Ui, ui::gamepad};

pub fn render(ui: Ui) -> Element {
    let mut live = ui.live;
    let mut set = ui.set;
    let open = (live.sheet_open)();
    let panel = (set.panel_open)();
    let more = (live.win_open)();
    let pad = (set.pad_on)();
    let on = (live.session_on)();
    // More turns into the keyboard only when that is certainly what is wanted next.
    let type_now = on && (live.text_wanted)() && !(live.osk_on)();

    rsx! {
        // No `idle` class here: the bar starts visible and js/bar.js fades it on a timer.
        div { id: "wado-bar",
            div { class: "bargroup",
                if type_now {
                    // A `label`, not a `button`: Android raises the soft keyboard only for a
                    // focus inside the user gesture, and label activation focuses its `for`
                    // target natively. See `js/osk.js`.
                    label {
                        r#for: "wado-osk",
                        class: "barbtn nav",
                        title: "Keyboard",
                        "aria-label": "Keyboard",
                        "⌨"
                    }
                } else {
                    button {
                        class: if more { "barbtn nav active" } else { "barbtn nav" },
                        title: "More",
                        "aria-label": "More",
                        "aria-expanded": "{more}",
                        onclick: move |_| live.win_open.set(!more),
                        "⋯"
                    }
                }
                if more {
                    div { class: "barpop barsheet",
                        // Closes after any pick: the sheet covers the video it acts on.
                        onclick: move |_| live.win_open.set(false),
                        label {
                            r#for: "wado-osk",
                            class: if !on { "barbtn off" }
                                   else if (live.osk_on)() { "barbtn active" }
                                   else { "barbtn" },
                            title: "Keyboard",
                            "aria-label": "Keyboard",
                            "⌨"
                        }
                        button {
                            class: "barbtn settings-only",
                            title: "Settings",
                            "aria-label": "Settings",
                            onclick: move |_| live.sheet_open.set(!open),
                            "⚙"
                        }
                        button {
                            class: "barbtn docked-only",
                            title: if panel { "Hide settings panel" } else { "Show settings panel" },
                            "aria-label": "Settings panel",
                            onclick: move |_| set.panel_open.set(!panel),
                            if panel { "⟨" } else { "⟩" }
                        }
                        button {
                            class: "barbtn",
                            title: "Fullscreen",
                            "aria-label": "Fullscreen",
                            onclick: move |_| {
                                // The session's own dimensions, so the orientation lock matches
                                // the output rather than however the phone is held.
                                let c = cfg::build(ui);
                                bridge::call(format!(
                                    "window.__wado.toggleFullscreen({}, {});", c.width, c.height
                                ));
                            },
                            "⛶"
                        }
                        // Close / maximize / minimize moved to a long-press on the app's own
                        // icon in the switcher dial (js/switcher.js), which names the app.
                        button {
                            class: "barbtn",
                            title: "Console (shell and log)",
                            "aria-label": "Console",
                            disabled: !on,
                            onclick: move |_| {
                                let open = !(live.console_open)();
                                live.console_open.set(open);
                                // Revealing the terminal is what makes it re-measure (and
                                // starts the shell the first time).
                                if open && (live.console_tab)() == "shell" {
                                    bridge::call("window.__wado.ptyShow();".to_string());
                                }
                            },
                            "❯_"
                        }
                        button {
                            class: if pad { "barbtn active" } else { "barbtn" },
                            title: "On-screen gamepad",
                            "aria-label": "On-screen gamepad",
                            "aria-pressed": "{pad}",
                            disabled: !on,
                            onclick: move |_| {
                                let mut set = set;
                                set.pad_on.set(!pad);
                                gamepad::apply(ui);
                            },
                            "🎮"
                        }
                        // Pointer lock. No `onclick`: `js/input_lock.js` catches the click on
                        // this id in the capture phase, inside the gesture it needs.
                        button {
                            id: "wado-lock",
                            class: if (live.pointer_lock)() { "barbtn active" } else { "barbtn" },
                            title: "Take the mouse (for games) — Escape releases it",
                            "aria-label": "Take the mouse",
                            "aria-pressed": "{(live.pointer_lock)()}",
                            disabled: !on,
                            "🎯"
                        }
                        // A fresh peer connection; the session survives.
                        button {
                            class: "barbtn",
                            title: "Resync video (rebuilds the connection, keeps the session)",
                            "aria-label": "Resync video",
                            disabled: !on,
                            onclick: move |_| bridge::call("window.__wado.resync();".to_string()),
                            "⟳"
                        }
                    }
                }
            }
            button {
                class: "barbtn nav",
                title: "Apps",
                "aria-label": "Apps",
                disabled: !on,
                onclick: move |_| {
                    let open = !(live.drawer_open)();
                    live.drawer_open.set(open);
                    // Re-asked on every open: the list says which apps are running *now*.
                    if open {
                        bridge::call("window.__wado.requestApps();".to_string());
                    }
                },
                "○"
            }
            button {
                class: "barbtn nav",
                title: "Back",
                "aria-label": "Back",
                disabled: !on,
                onclick: move |_| bridge::call("window.__wado.back();".to_string()),
                "◁"
            }
        }
    }
}
