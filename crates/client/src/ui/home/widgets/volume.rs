//! The Volume widget: its computer's speakers — mute, and how loud.

use dioxus::prelude::*;
use wado_protocol::HostAction;

use crate::ui::{
    host,
    widgets::{Icon, Slider},
};

use super::shell::{asking, Ctx};

#[component]
pub fn Volume(cx: Ctx) -> Element {
    let Some(h) = cx.r.host.clone() else {
        return asking(&cx.name);
    };
    let Some(s) = host::speaker(&h).cloned() else {
        return rsx! { div { class: "wnote quiet", Icon { name: "sound" } "{cx.name} has no speakers wado can see." } };
    };
    let (ui, k1, k2) = (cx.ui, cx.w.host.clone(), cx.w.host.clone());
    let name = cx.name.clone();
    rsx! {
        div { class: "volrow",
            button {
                class: if s.muted { "iconbtn muted" } else { "iconbtn" },
                "aria-label": if s.muted { "Unmute {name}" } else { "Mute {name}" },
                "aria-pressed": "{s.muted}",
                onclick: move |_| host::act_on(ui, &k1, HostAction::SinkMute { id: s.id, muted: !s.muted }),
                span { key: "{s.muted}", class: "glyph", Icon { name: if s.muted { "mute" } else { "sound" } } }
            }
            Slider {
                label: "Volume",
                value: s.volume.min(1.0) as f64,
                shown: if s.muted { "muted".to_string() } else { String::new() },
                onchange: move |v: f64| host::act_on(ui, &k2, HostAction::SinkVolume { id: s.id, volume: v as f32 }),
            }
        }
    }
}
