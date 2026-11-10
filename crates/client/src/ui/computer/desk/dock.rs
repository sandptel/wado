//! The Apps card, in place of Sessions on a Mac with `desk_apps`: a shelf like the macOS Dock —
//! its own Dock's apps in their order, a divider, then what else runs — and "All apps". The link
//! test stays under it.

use dioxus::prelude::*;

use super::tile::{Menu, Tile};
use crate::{
    bridge,
    reach::Reach,
    route::{self, View},
    state::Ui,
    ui::{computer::linkmeter::LinkMeter, pages::Page, widgets::Icon},
};

#[component]
pub fn Dock(ui: Ui, r: Reach) -> Element {
    let live = ui.live;
    // Asked every 3 s while it is looked at: this page, or the Apps page over it.
    use_effect(move || {
        let watch = !(live.session_on)()
            && matches!((live.view)(), View::Computer(_) | View::Sub(_, Page::Apps));
        bridge::call(format!("window.__wado.deskAppsWatch({watch});"));
    });
    use_drop(|| bridge::call("window.__wado.deskAppsWatch(false);".to_string()));
    let here = matches!((live.view)(), View::Computer(_));
    // Offline: the apps it had, dimmed and with no state, saying when they open.
    let apps = if r.online {
        live.apps.read().clone()
    } else {
        super::unknown(&live.apps.read())
    };
    let (kept, extra) = super::dock(&apps);

    rsx! {
        section { class: "sx o2 dapps", style: "--n:2", "aria-label": "Apps",
            div { class: "sect",
                span { "Apps" }
                button { onclick: move |_| route::open_page(ui, Page::Apps), "All apps" }
            }
            if !r.online && kept.is_empty() && extra.is_empty() {
                div { class: "npempty", Icon { name: "apps" } "Its apps show here once it has been connected." }
            } else if kept.is_empty() && extra.is_empty() {
                div { class: "dshelf dwait", "Asking for its apps…" }
            } else {
                if !r.online {
                    p { class: "dnotlinked", Icon { name: "alert" } "Not connected — these open once it is back." }
                }
                div { class: if r.online { "dshelf" } else { "dshelf off" }, role: "group", "aria-label": "Dock",
                    for a in kept.iter() {
                        Tile { key: "{a.exec}", app: (*a).clone(), named: false }
                    }
                    if !kept.is_empty() && !extra.is_empty() {
                        i { class: "dsep", "aria-hidden": "true" }
                    }
                    for a in extra.iter() {
                        Tile { key: "{a.exec}", app: (*a).clone(), named: false }
                    }
                }
            }
            LinkMeter { ui, intent: "start".to_string() }
            if here {
                Menu { ui }
            }
        }
    }
}
