//! The workspace bar: a pill per workspace in use — its number, and on the one showing, its
//! apps' icons. Tap a number to switch, an icon to focus, hold an icon for its sheet (move to
//! another workspace, maximise, close). `+` opens an empty workspace.
//!
//! Plain Dioxus over the compositor's own window list (`WindowList`), so it can only ever show
//! what the compositor has — the old JS dial animated per frame and drifted from it.

use dioxus::prelude::*;
use serde_json::json;
use wado_protocol::{AppEntry, WindowInfo};

use crate::{bridge, state::Ui, ui::widgets::Icon};

fn act(a: serde_json::Value) {
    bridge::call(format!("window.__wado.windowAction({});", bridge::js(&a)));
}

/// The icon for a window: its app's, by `app_id`, or `None` for a letter tile.
fn icon_of(apps: &[AppEntry], w: &WindowInfo) -> Option<String> {
    let id = w.app_id.to_ascii_lowercase();
    apps.iter()
        .find(|a| a.app_ids.iter().any(|x| x.to_ascii_lowercase() == id))
        .and_then(|a| a.icon.clone())
}

fn letter(w: &WindowInfo) -> String {
    let src = if w.app_id.is_empty() {
        &w.title
    } else {
        &w.app_id
    };
    let base = src.rsplit('.').next().unwrap_or(src);
    base.chars()
        .next()
        .map_or("?".into(), |c| c.to_uppercase().to_string())
}

/// The workspaces to draw: every one with a window, and the one showing even if empty.
pub fn in_use(windows: &[WindowInfo], active: u32) -> Vec<u32> {
    let mut v: Vec<u32> = windows.iter().map(|w| w.workspace.max(1)).collect();
    v.push(active.max(1));
    v.sort_unstable();
    v.dedup();
    v
}

fn win_icon(apps: &[AppEntry], w: &WindowInfo) -> Element {
    let label = if w.title.is_empty() {
        w.app_id.clone()
    } else {
        w.title.clone()
    };
    let id = w.id;
    rsx! {
        button {
            key: "w{id}",
            class: if w.focused { "wsapp on" } else { "wsapp" },
            "data-win": "{id}",
            title: "{label}",
            "aria-label": "{label}",
            onclick: move |_| act(json!({ "focus": { "id": id } })),
            if let Some(src) = icon_of(apps, w) {
                img { src: "{src}", alt: "" }
            } else {
                span { class: "wsletter", "{letter(w)}" }
            }
        }
    }
}

pub fn bar(ui: Ui) -> Element {
    let (windows, active) = (ui.live.windows)();
    let apps = (ui.live.apps)();
    let used = in_use(&windows, active);
    let next = used.last().copied().unwrap_or(1) + 1;
    rsx! {
        div { class: "wsbar", role: "tablist", "aria-label": "Workspaces",
            for n in used {
                {
                    let here: Vec<&WindowInfo> = windows.iter().filter(|w| w.workspace.max(1) == n).collect();
                    let on = n == active.max(1);
                    let count = here.len();
                    rsx! {
                        div { key: "ws{n}", class: if on { "wspill on" } else { "wspill" },
                            button {
                                class: "wsnum",
                                role: "tab",
                                "aria-selected": "{on}",
                                "aria-label": "Workspace {n}, {count} windows",
                                onclick: move |_| act(json!({ "workspace": { "n": n } })),
                                "{n}"
                            }
                            if on {
                                for w in here.iter().take(5) { {win_icon(&apps, w)} }
                            } else if count > 0 {
                                span { class: "wsdots", "aria-hidden": "true",
                                    for i in 0..count.min(3) { i { key: "d{i}" } }
                                }
                            }
                        }
                    }
                }
            }
            button {
                class: "wsnum add",
                title: "New workspace",
                "aria-label": "New workspace",
                onclick: move |_| act(json!({ "workspace": { "n": next } })),
                Icon { name: "plus" }
            }
        }
    }
}

/// The sheet a long press opens for one window.
pub fn sheet(ui: Ui) -> Element {
    let mut live = ui.live;
    let Some(id) = (live.win_menu)() else {
        return rsx! {};
    };
    let (windows, active) = (live.windows)();
    let Some(w) = windows.iter().find(|w| w.id == id).cloned() else {
        return rsx! {};
    };
    let mut close = move || live.win_menu.set(None);
    let mut go = move |a: serde_json::Value| {
        act(a);
        live.win_menu.set(None);
    };
    let used = in_use(&windows, active);
    let next = used.last().copied().unwrap_or(1) + 1;
    let here = w.workspace.max(1);
    let title = if w.title.is_empty() {
        w.app_id.clone()
    } else {
        w.title.clone()
    };
    rsx! {
        div { class: "scrim", onclick: move |_| close() }
        div { class: "winsheet", role: "dialog", "aria-label": "{title}",
            div { class: "grab" }
            b { class: "winsheet-title", "{title}" }
            div { class: "cardhead", "Move to workspace" }
            div { class: "wsmove",
                for n in used.into_iter().chain(std::iter::once(next)) {
                    button {
                        key: "m{n}",
                        class: if n == here { "wsnum on" } else { "wsnum" },
                        disabled: n == here,
                        "aria-label": if n == next { "A new workspace" } else { "Workspace {n}" },
                        onclick: move |_| go(json!({ "move_to_workspace": { "id": id, "n": n, "follow": false } })),
                        if n == next { Icon { name: "plus" } } else { "{n}" }
                    }
                }
            }
            div { class: "winacts",
                button { class: "chip", onclick: move |_| go(json!({ "move_to_workspace": { "id": id, "n": next, "follow": true } })),
                    Icon { name: "right" } "Take it to a new workspace"
                }
                button { class: "chip", onclick: move |_| { act(json!({ "focus": { "id": id } })); go(json!("maximize")); },
                    Icon { name: "max" } "Maximise"
                }
                button { class: "chip bad", onclick: move |_| { act(json!({ "focus": { "id": id } })); go(json!("close")); },
                    Icon { name: "x" } "Close"
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(id: u64, ws: u32) -> WindowInfo {
        WindowInfo {
            id,
            title: String::new(),
            app_id: "org.gnome.Nautilus".into(),
            focused: false,
            workspace: ws,
        }
    }

    #[test]
    fn in_use_keeps_the_active_one_and_sorts() {
        assert_eq!(in_use(&[w(1, 3), w(2, 1), w(3, 3)], 5), vec![1, 3, 5]);
        assert_eq!(in_use(&[], 0), vec![1]);
        assert_eq!(letter(&w(1, 1)), "N");
    }
}
