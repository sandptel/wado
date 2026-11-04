//! The one sheet of the new surfaces (the app's `widgets/sheet.dart`), per form: on a phone a bottom
//! sheet that rises on the spring; on a tablet or desktop a centred card that scales in — the
//! animation each kind of screen expects. Esc or the scrim closes it; focus starts inside.

use dioxus::prelude::*;

use crate::state::Ui;

/// The shared form (`wado_ux::form`) from the console's own reading of the screen.
pub fn form(ui: Ui) -> wado_ux::form::Form {
    use crate::ui::surround::layout::Form as F;
    match (ui.live.stage)().form {
        F::Desk => wado_ux::form::Form::Desktop,
        F::TabPort | F::TabLand => wado_ux::form::Form::Tablet,
        _ => wado_ux::form::Form::Phone,
    }
}

#[component]
pub fn Sheet(title: String, sub: Option<String>, on_close: EventHandler<()>, children: Element) -> Element {
    rsx! {
        div {
            class: "wscrim",
            onclick: move |_| on_close.call(()),
            onkeydown: move |e: KeyboardEvent| if e.key() == Key::Escape { on_close.call(()) },
            div {
                class: "wsheet",
                role: "dialog",
                "aria-modal": "true",
                "aria-label": "{title}",
                tabindex: "-1",
                onmounted: move |e| { let _ = e.data().set_focus(true); },
                onclick: move |e| e.stop_propagation(),
                div { class: "wgrab", "aria-hidden": "true" }
                div { class: "whead",
                    b { "{title}" }
                    if let Some(s) = sub { span { "{s}" } }
                }
                {children}
            }
        }
    }
}
