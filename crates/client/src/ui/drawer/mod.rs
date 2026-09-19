//! The app drawer: a sheet of application icons over the video.
//!
//! **Why it is not in the settings panel.** The launcher used to live there, which meant
//! starting an application cost open-panel → scroll past the session settings → type → tap
//! Launch. On a phone that is four deliberate actions for the thing people do most. Here it is
//! one: ⊞ on the bar, then the icon.
//!
//! **One input, not two.** The box at the top is both the filter and the free-text command
//! line, exactly as the old launcher's was — typing narrows the grid, and whatever is in the
//! box can always be run as-is. A separate search field would mean two places to look and a
//! rule about which one wins.
//!
//! **Tap launches; long-press fills the box.** Tapping is the common case and should not cost
//! a second tap on a Launch button. Long-press is for the other one — taking an application's
//! command and adding a flag to it before running. It is wired to `contextmenu`, which is what
//! a long press raises on touch and a right-click raises on a desktop; nothing else in the
//! stage uses that event.
//!
//! Split three ways: this file is the sheet and what goes in it, [`tile`] is one application
//! and how it responds to a press, [`recent`] is the pure join of stored commands against the
//! live list.
//!
//! ponytail: no favourites, no categories, no per-app state. The grid is the app list filtered
//! by a string, and recents is the last four commands. Pinning is the upgrade path if anyone
//! misses it.

pub mod recent;
pub mod tile;

use dioxus::prelude::*;
use wado_protocol::AppEntry;

use crate::{actions, state::Ui};

/// Most tiles rendered at once. Filtering is how you find something in a list of three
/// hundred; rendering all three hundred just makes the sheet scroll forever and the DOM heavy
/// on a phone.
const MAX_TILES: usize = 40;

pub fn render(ui: Ui) -> Element {
    let mut live = ui.live;
    let mut s = ui.set;
    let open = (live.drawer_open)();
    let command = (s.command)();
    let query = command.trim().to_lowercase();

    let apps = live.apps.read().clone();
    let matches: Vec<AppEntry> = apps
        .iter()
        .filter(|a| {
            query.is_empty()
                || a.name.to_lowercase().contains(&query)
                || a.exec.to_lowercase().contains(&query)
        })
        .take(MAX_TILES)
        .cloned()
        .collect();

    let recent = recent::entries(&s.recent.read(), &apps);

    // The box holds something no tile offers — a command with flags, or a typo. Running it is
    // the only thing left to do with it, so the button appears rather than sitting there
    // greyed out above a full grid.
    let free_text = !command.trim().is_empty() && !matches.iter().any(|a| a.exec == command.trim());

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

            input {
                r#type: "text",
                class: "drawersearch",
                value: "{command}",
                placeholder: "search apps, or type a command",
                oninput: move |e| s.command.set(e.value()),
            }

            if free_text {
                button {
                    class: "wide drawerrun",
                    onclick: move |_| {
                        actions::launch_command(ui, &(s.command)());
                        live.drawer_open.set(false);
                    },
                    "Run “{command.trim()}”"
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
                            "No application list from the server — the box above still runs any command."
                        } else {
                            "Nothing matches. The box above still runs any command."
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
