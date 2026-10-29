//! Display & session: the session running on this computer (and starting another), its shape,
//! frame rate, scale and tuning — and, under its own head, what this computer holds for every
//! device: stream caps, shortcuts, three-finger swipes, keyboard policy.

use dioxus::prelude::*;

use crate::{
    state::Ui,
    ui::{
        home::sessions,
        pages::{display, host, input},
        widgets::{Icon, When, WhenBadge},
    },
};

pub fn render(ui: Ui) -> Element {
    let name = crate::profile::current_name(ui);
    let (_, list) = (ui.live.sessions)();
    let on = (ui.live.session_on)();
    rsx! {
        if !list.is_empty() {
            div { class: "card",
                div { class: "cardhead", "Running session" span { class: "whenbadge host", "on {name}" } }
                for s in list.iter() {
                    div { key: "{s.instance}", class: "sessionhead",
                        div { class: "sessionshape", title: "{s.width}×{s.height}",
                            span { style: "{sessions::shape_style(s.width, s.height)}" }
                        }
                        div { class: "navtext",
                            b { "{sessions::headline(&s.apps)}" }
                            small { class: "mono", "{s.width}×{s.height} · {s.fps} fps · started {sessions::ago(s.age_s)}" }
                        }
                    }
                }
                p { class: "why", "Shape, frame rate, scale and tuning change it in place — its apps stay open." }
                if !on {
                    button { class: "btn", onclick: move |_| sessions::new(ui), Icon { name: "plus" } "Start another session" }
                }
            }
        }
        {crate::ui::scoped(ui, "display", display::render)}
        if host::relay_only(ui) {
            div { class: "sect", span { "For every device on {name}" } }
            {host::notes(ui)}
            div { class: "card",
                div { class: "cardhead", "Limits" WhenBadge { when: When::Host } }
                {host::caps(ui)}
                {host::input_everyone(ui)}
            }
            {input::swipes(ui)}
            {crate::ui::scoped(ui, "shortcuts", host::shortcuts)}
        } else {
            div { class: "card", p { class: "why", "What this computer holds for every device travels over the relay. Connect via relay to change it here — or edit ~/.config/wado/config.kdl on {name}." } }
        }
        {input::keyboard(ui)}
    }
}
