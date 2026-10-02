//! The console: the daemon's shells as tabs, and the server log, in one sheet over the picture.
//!
//! Over the picture rather than beside it — as a sibling it took height off the stream. One
//! sheet with tabs rather than separate panels, because a phone has room for one at a time.
//!
//! The terminals are real PTYs on the daemon, several of them, and they outlive this page (see
//! `server::shells`). xterm.js owns everything inside `#wado-term` — one pane per shell, made by
//! `js/pty.js` — and Dioxus never diffs it. For the same reason the sheet is **hidden with a
//! class, not unmounted**: removing the node would take every terminal's screen with it.

use dioxus::prelude::*;

use crate::{bridge, state::Ui, ui::widgets::Icon};

const KEYS: &[(&str, &str)] = &[
    ("esc", "esc"),
    ("tab", "tab"),
    ("ctrl", "ctrl"),
    ("alt", "alt"),
    ("left", "←"),
    ("up", "↑"),
    ("down", "↓"),
    ("right", "→"),
    ("pipe", "|"),
    ("tilde", "~"),
    ("slash", "/"),
    ("dash", "-"),
    ("home", "home"),
    ("end", "end"),
    ("pgup", "pgup"),
    ("pgdn", "pgdn"),
];

/// Open the console on its shells, starting a local one if there is none.
pub fn open_shells(ui: Ui) {
    let mut live = ui.live;
    live.console_tab.set("shell".to_string());
    live.console_open.set(true);
    if live.shells.read().is_empty() {
        bridge::call("window.__wado.shellNew(null);".to_string());
    } else {
        bridge::call("window.__wado.ptyShow();".to_string());
    }
}

pub fn render(ui: Ui) -> Element {
    let mut live = ui.live;
    let open = (live.console_open)();
    let shell = (live.console_tab)() == "shell";
    let shells = (live.shells)();
    let hosts = (live.shell_hosts)();
    let active = (live.shell_active)();
    let (ctrl, alt) = (live.shell_mods)();
    let logs = live.logs.read().clone();
    let mut picker = use_signal(|| false);

    rsx! {
        section {
            id: "console",
            class: if open { "open" } else { "shut" },

            div { class: "grab", "data-dismiss": "1", "aria-label": "Close", onclick: move |_| live.console_open.set(false) }
            div { class: "consoletabs",
                for s in shells.iter().cloned() {
                    div {
                        key: "{s.id}",
                        class: if shell && s.id == active { "ctab on" } else { "ctab" },
                        button {
                            class: if s.alive { "ctabname" } else { "ctabname dead" },
                            onclick: move |_| {
                                live.console_tab.set("shell".to_string());
                                live.shell_active.set(s.id);
                                bridge::call(format!("window.__wado.shellShow({});", s.id));
                            },
                            span { class: if s.host.is_some() { "cdot ssh" } else { "cdot" } }
                            "{s.title}"
                        }
                        button {
                            class: "ctabx",
                            "aria-label": "Close {s.title}",
                            onclick: move |_| bridge::call(format!("window.__wado.shellClose({});", s.id)),
                            Icon { name: "x" }
                        }
                    }
                }
                button { class: "ctab ctabplus", "aria-label": "New shell", onclick: move |_| picker.set(!picker()),
                    Icon { name: "plus" }
                }
                button {
                    class: if shell { "ctab" } else { "ctab on" },
                    onclick: move |_| live.console_tab.set("logs".to_string()),
                    "Logs"
                }
                span { class: "ctabgap" }
                button {
                    class: "ctab cclose",
                    "aria-label": "Close console",
                    onclick: move |_| live.console_open.set(false),
                    Icon { name: "down" }
                }
            }

            if picker() {
                div { class: "hostpick",
                    button { class: "navrow", onclick: move |_| { picker.set(false); bridge::call("window.__wado.shellNew(null);".to_string()); },
                        span { class: "disc", style: "--hue:var(--base0B)", Icon { name: "term" } }
                        span { class: "navtext", b { "This computer" } small { "a login shell" } }
                    }
                    for h in hosts.iter().cloned() {
                        button {
                            key: "{h}",
                            class: "navrow",
                            onclick: move |_| {
                                picker.set(false);
                                bridge::call(format!("window.__wado.shellNew({});", bridge::js(&h)));
                            },
                            span { class: "disc", style: "--hue:var(--base0E)", Icon { name: "server" } }
                            span { class: "navtext", b { "{h}" } small { "ssh {h} — from this computer's ~/.ssh/config" } }
                        }
                    }
                    if hosts.is_empty() {
                        p { class: "why", "Add Host entries to ~/.ssh/config on the computer running wado, and they appear here." }
                    }
                }
            }

            // Always in the tree, shown only on a shell tab. Empty on purpose — js/pty.js
            // builds one pane per shell inside it and owns them from then on.
            div { id: "wado-term", class: if shell { "termshow" } else { "termhide" } }

            if shell && active != 0 {
                div { class: "keyrow",
                    for (k, label) in KEYS.iter() {
                        button {
                            key: "{k}",
                            class: if (*k == "ctrl" && ctrl) || (*k == "alt" && alt) { "on" } else { "" },
                            // Keep the terminal's focus — and the phone keyboard — up.
                            onmousedown: move |e| e.prevent_default(),
                            onclick: move |_| bridge::call(format!("window.__wado.shellKey({});", bridge::js(k))),
                            "{label}"
                        }
                    }
                }
            }

            if !shell {
                div { id: "wado-logwrap", class: "consolebody",
                    for (i, line) in logs.iter().enumerate() {
                        div { key: "{i}", class: "logline l-{line.level}",
                            span { class: "lts", "{line.ts}" }
                            " {line.text}"
                        }
                    }
                }
            }
        }
    }
}
