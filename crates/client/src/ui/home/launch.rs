//! "The poster becomes the game" (the app's `ui/play/launch.dart`): the layer `js/launch.js` grows
//! out of the poster tapped — the whole screen on a phone, a centred card on a big one — with the
//! connection's hops told as plain steps. When the first frame lands it fades and the picture is
//! underneath; a hop that fails names itself, with Close and Try again.

use dioxus::prelude::*;

use crate::{bridge, state::Ui, ui::widgets::Icon};

pub fn render(ui: Ui) -> Element {
    let mut live = ui.live;
    // Done: the picture is up, or the session went away. Fade, then gone.
    use_effect(move || {
        if (live.launching)().is_some() && ((live.fps)().is_some() || !(live.session_on)()) {
            bridge::call(
                "var g=document.getElementById('glaunch'); if(g) g.classList.add('out');"
                    .to_string(),
            );
            spawn(async move {
                bridge::sleep(320).await;
                live.launching.set(None);
            });
        }
    });
    // Never a layer left over the session: it goes after 45 s whatever happens.
    use_future(move || async move {
        bridge::sleep(45_000).await;
        live.launching.set(None);
    });
    // Hooks first, always in the same order; then whether there is anything to show.
    let Some((name, hue)) = (live.launching)() else {
        return rsx! {};
    };
    let stage = (live.conn_stage)();
    let err = (live.conn_error)();
    let video = (live.fps)().is_some();
    let computer = crate::profile::current_name(ui);
    let steps = [
        (format!("Reaching {computer}"), stage >= 1),
        ("Making room for this screen".to_string(), stage >= 3),
        (format!("Opening {name}"), stage >= 4),
        ("Here it comes".to_string(), video),
    ];
    let now = steps.iter().position(|s| !s.1).unwrap_or(steps.len());
    rsx! {
        div { class: "glaunchscrim", "aria-hidden": "true" }
        div { id: "glaunch", role: "status", "aria-live": "polite", style: "--h: var(--base{hue})",
            div { class: "glin",
                span { class: "gletter big", "{name.chars().next().unwrap_or('?')}" }
                b { "{name}" }
                ol { class: "ghops",
                    for (i, (words, done)) in steps.iter().enumerate() {
                        li {
                            key: "{i}",
                            class: if *done { "done" } else if i == now && !err.is_empty() { "failed" } else if i == now { "now" } else { "" },
                            span { class: "gm", if *done { Icon { name: "check" } } }
                            "{words}"
                        }
                    }
                }
                if !err.is_empty() {
                    p { class: "gerr", "{err}" }
                    div { class: "grow2",
                        button { class: "btn", onclick: move |_| live.launching.set(None), "Close" }
                        button { class: "btn primary", onclick: move |_| crate::actions::start(ui), "Try again" }
                    }
                }
            }
        }
    }
}
