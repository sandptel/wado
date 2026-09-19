//! The app drawer: a sheet of application icons over the video.
//!
//! **Why it is not in the settings panel.** The launcher used to live there, which meant
//! starting an application cost open-panel → scroll past the session settings → type → tap
//! Launch. On a phone that is four deliberate actions for the thing people do most. Here it is
//! one: ⊞ on the bar, then the icon.
//!
//! **One input, not two.** The box at the top is both the filter and the free-text command
//! line, exactly as the old launcher's was — typing narrows the grid, and whatever is in the
//! box can always be run as-is, with ▶ or with Enter. A separate search field would mean two
//! places to look and a rule about which one wins.
//!
//! **Tap launches; long-press fills the box.** Tapping is the common case and should not cost
//! a second tap on a Launch button. Long-press is for the other one — taking an application's
//! command and adding a flag to it before running. It is wired to `contextmenu`, which is what
//! a long press raises on touch and a right-click raises on a desktop; nothing else in the
//! stage uses that event.
//!
//! **👁 lists everything installed.** A desktop-file scan turns up far more than a menu should
//! show: on the machine this was measured on, 143 launchable entries of which 71 are marked
//! `NoDisplay` — MIME handlers, setup helpers, per-URL-scheme stubs — and another 9 that no
//! installed theme has an icon for. Both sets are hidden by default and both come back with
//! the eye, because neither mark means *unlaunchable*; they mean *not normally interesting*,
//! which is a judgement the person looking at the screen gets to overrule.
//!
//! Split four ways: this file is the sheet and what goes in it, [`tile`] is one application
//! and how it responds to a press, [`recent`] is the pure join of stored commands against the
//! live list, [`run`] is what the typed text means.
//!
//! ponytail: no favourites, no categories, no per-app state. The grid is the app list filtered
//! by a string, and recents is the last four commands. Pinning is the upgrade path if anyone
//! misses it.

pub mod recent;
pub mod run;
pub mod tile;

use dioxus::prelude::*;
use wado_protocol::AppEntry;

use crate::{actions, state::Ui};

/// Whether an entry belongs in the default list.
///
/// Two marks, one rule. `hidden` is the desktop file's own `NoDisplay`/`Hidden` — the author
/// saying this is not a menu item. A missing icon is the weaker signal, and it is here because
/// in practice it catches the same kind of thing: packaging leftovers and command-line tools
/// with a `.desktop` file nobody styled. Either one is enough to bury an entry; 👁 overrules
/// both, and search never reaches past them unless it is on.
fn listable(app: &AppEntry) -> bool {
    !app.hidden && app.icon.is_some()
}

/// Most tiles rendered at once — a guard against a pathological machine, not curation. A
/// normal desktop has one to three hundred desktop files and every one of them should be
/// reachable by scrolling; filtering is for finding something, not for seeing what is there.
const MAX_TILES: usize = 400;

pub fn render(ui: Ui) -> Element {
    let mut live = ui.live;
    let mut s = ui.set;
    let open = (live.drawer_open)();
    let command = (s.command)();
    let query = command.trim().to_lowercase();
    let show_hidden = (s.show_hidden)();

    let apps = live.apps.read().clone();
    let found: Vec<&AppEntry> = apps
        .iter()
        .filter(|a| {
            query.is_empty()
                || a.name.to_lowercase().contains(&query)
                || a.exec.to_lowercase().contains(&query)
        })
        .collect();
    // Counted over everything the query found, not over what is rendered, so the number on the
    // eye is the number of entries it will actually reveal.
    let buried = found.iter().filter(|a| !listable(a)).count();
    let matches: Vec<AppEntry> = found
        .into_iter()
        .filter(|a| show_hidden || listable(a))
        .take(MAX_TILES)
        .cloned()
        .collect();

    let recent = recent::entries(&s.recent.read(), &apps);

    let eye_title = if show_hidden {
        "Show only applications".to_string()
    } else if buried == 1 {
        "Show everything installed (1 more)".to_string()
    } else {
        format!("Show everything installed ({buried} more)")
    };
    // Enter and ▶ do the same thing, so they run the same closure body over the same list.
    let for_click = apps.clone();
    let for_enter = apps.clone();
    let mut go = move |apps: &[AppEntry]| {
        if let Some(cmd) = run::resolve(&(s.command)(), apps) {
            actions::launch_command(ui, &cmd);
            live.drawer_open.set(false);
        }
    };
    let mut go_click = go.clone();

    rsx! {
        section {
            id: "drawer",
            class: if open { "open" } else { "shut" },

            // Tap-anywhere-on-the-lip to dismiss: the scrim belongs to the settings sheet and
            // the video underneath must stay tappable while the drawer is open.
            button {
                class: "drawerhandle",
                "aria-label": "Close app drawer",
                onclick: move |_| live.drawer_open.set(false),
            }

            // The box and its two verbs on one line. They belong to the box — one reveals what
            // the box is hiding from you, the other runs what you put in it — and a row of
            // full-width buttons under it would push the grid off a phone screen.
            div { class: "drawerbar",
                input {
                    r#type: "text",
                    class: "drawersearch",
                    value: "{command}",
                    placeholder: "search apps, or type a command",
                    // A search field on a phone gets a "go" key instead of a newline, which is
                    // the whole reason Enter has to mean something here.
                    oninput: move |e| s.command.set(e.value()),
                    onkeydown: move |e| {
                        if e.key() == Key::Enter {
                            e.prevent_default();
                            go_click(&for_enter);
                        }
                    },
                }
                button {
                    class: if show_hidden { "barbtn active" } else { "barbtn" },
                    title: "{eye_title}",
                    "aria-label": "{eye_title}",
                    "aria-pressed": "{show_hidden}",
                    disabled: buried == 0 && !show_hidden,
                    onclick: move |_| s.show_hidden.set(!show_hidden),
                    "👁"
                }
                button {
                    class: "barbtn",
                    title: "Run what is in the box",
                    "aria-label": "Run",
                    disabled: command.trim().is_empty(),
                    onclick: move |_| go(&for_click),
                    "▶"
                }
            }

            div { class: "drawerbody",
                if !recent.is_empty() && query.is_empty() {
                    p { class: "hint drawerlabel", "Recent" }
                    div { class: "appgrid",
                        for app in recent.iter() {
                            {tile::render(ui, app)}
                        }
                    }
                    p { class: "hint drawerlabel", "All apps" }
                }

                if matches.is_empty() {
                    p { class: "hint",
                        if apps.is_empty() {
                            "No application list from the server — ▶ still runs whatever you type."
                        } else if buried > 0 {
                            "Nothing here matches — 👁 searches everything installed too. ▶ runs what you typed."
                        } else {
                            "Nothing matches. ▶ still runs what you typed."
                        }
                    }
                } else {
                    div { class: "appgrid",
                        for app in matches.iter() {
                            {tile::render(ui, app)}
                        }
                    }
                }
            }
        }
    }
}
