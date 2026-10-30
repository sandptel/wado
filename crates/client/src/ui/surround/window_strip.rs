//! The window strip: the focused window, as the bezel's chin. Its title opens the window sheet
//! (the tap path to what a long press on a dock icon opens), then Minimise, Maximise and Close.
//!
//! Only what `WindowAction` can do today is shown: there is no free resize, so there is no resize
//! control (spec §11.8). Maximise is a toggle on the compositor and the window list does not say
//! which state a window is in, so the button is one label ("Maximise or restore").

use dioxus::prelude::*;
use serde_json::json;

use crate::{
    state::Ui,
    ui::{widgets::Icon, workspaces},
};

pub fn render(ui: Ui) -> Element {
    let mut live = ui.live;
    let (windows, _) = (live.windows)();
    let apps = (live.apps)();
    let focused = windows.iter().find(|w| w.focused).cloned();
    // "No window open" only once there is a picture to have no window in: before that, the
    // stage's own state says what is happening.
    if (live.stage)().out_w <= 0.0 {
        return rsx! {
            div { class: "cstrip empty", role: "group", "aria-label": "Windows" }
        };
    }
    let Some(w) = focused else {
        return rsx! {
            div { class: "cstrip empty", role: "group", "aria-label": "No window",
                span { class: "ctitle none", "No window open" }
            }
        };
    };
    let id = w.id;
    let app = apps
        .iter()
        .find(|a| a.app_ids.iter().any(|x| x.eq_ignore_ascii_case(&w.app_id)))
        .map(|a| a.name.clone())
        .unwrap_or_else(|| {
            let base = w.app_id.rsplit('.').next().unwrap_or(&w.app_id);
            let mut c = base.chars();
            c.next()
                .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
                .unwrap_or_default()
        });
    let title = if w.title.is_empty() {
        app.clone()
    } else {
        w.title.clone()
    };
    let icon = workspaces::icon_of(&apps, &w);
    rsx! {
        div { class: "cstrip", role: "group", "aria-label": "Window: {app}",
            button {
                class: "ctitle",
                "aria-label": "Window: {app}, {title}. Open its window sheet.",
                onclick: move |_| live.win_menu.set(Some(id)),
                span { class: "cglyph",
                    if let Some(src) = icon {
                        img { src: "{src}", alt: "" }
                    } else {
                        span { class: "wsletter", "{workspaces::letter(&w)}" }
                    }
                }
                span { class: "ctt",
                    b { "{title}" }
                    small { "{app}" }
                }
            }
            button {
                class: "barbtn cwin",
                title: "Minimise {app}",
                "aria-label": "Minimise {app}",
                onclick: move |_| workspaces::act(json!("minimize")),
                Icon { name: "winmin" }
            }
            button {
                class: "barbtn cwin",
                title: "Maximise or restore {app}",
                "aria-label": "Maximise or restore {app}",
                onclick: move |_| workspaces::act(json!("maximize")),
                Icon { name: "winmax" }
            }
            button {
                class: "barbtn cwin close",
                title: "Close {app}",
                "aria-label": "Close {app}",
                onclick: move |_| workspaces::act(json!("close")),
                Icon { name: "x" }
            }
        }
    }
}
