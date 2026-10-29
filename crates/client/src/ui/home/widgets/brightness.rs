//! The Brightness widget: its computer's screen, in 10% steps, showing the real level.

use dioxus::prelude::*;
use wado_protocol::HostAction;

use crate::ui::{
    host,
    widgets::{Icon, Slider},
};

use super::shell::{asking, Ctx};

#[component]
pub fn Brightness(cx: Ctx) -> Element {
    let Some(h) = cx.r.host.clone() else {
        return asking(&cx.name);
    };
    let Some(b) = h.brightness else {
        return rsx! { div { class: "wnote quiet", Icon { name: "sun" } "{cx.name} has no screen brightness wado can set." } };
    };
    let (ui, k) = (cx.ui, cx.w.host.clone());
    rsx! {
        Slider {
            label: "Brightness",
            icon: "sun",
            value: b as f64,
            step: 0.1,
            ticks: 10,
            onchange: move |v: f64| host::act_on(ui, &k, HostAction::Brightness { level: v as f32 }),
        }
    }
}
