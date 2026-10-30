//! The Pocket Console: the picture inset at its true aspect, the controls around it
//! (`docs/superpowers/specs/2026-10-30-pocket-console-design.md`).
//!
//! One set of controls, two arrangements. The dock, the quick rail, the workspace bar, the link
//! badge and the people chip are the *same nodes* in both: in the console they sit at the slots
//! [`layout`] computes (written as CSS variables by `js/stage_fit.js`), in the cockpit they sit
//! where today's CSS puts them. `js/stage_morph.js` flies them between the two. What only the
//! console has — the bezel, the top band's words, the window strip, the face switcher and the
//! deck — is rendered here, always, and faded with `inert` in the cockpit, so a morph never
//! mounts or unmounts anything.
//!
//! | | |
//! |---|---|
//! | [`layout`] | the geometry: a pure function, unit-tested |
//! | [`top_band`] | the computer's name, or the software-encoding chip in its place |
//! | [`window_strip`] | the focused window: its title, Minimise, Maximise, Close |
//! | [`faces`] | the four input faces, and the cockpit rail's one |
//! | [`deck`] | what the active face puts under the thumbs |

pub mod deck;
pub mod faces;
pub mod layout;
pub mod top_band;
pub mod window_strip;

use dioxus::prelude::*;

use crate::{bridge, state::Ui};
use layout::{Face, Form, Frame, Insets};

/// The arrangement and what it is computed from. Owned by the page (`stage_fit.js`,
/// `stage_morph.js`, `console_prefs.js`) and mirrored here for rendering.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Stage {
    /// Full screen: today's Glass Cockpit. Otherwise the console.
    pub cockpit: bool,
    pub vw: f64,
    pub vh: f64,
    pub inset: Insets,
    pub form: Form,
    /// The video's own size, once a frame has decoded; 0 until then.
    pub out_w: f64,
    pub out_h: f64,
    /// `device { console { face } }`: `auto`, `pad`, `keys`, `quick` or `gamepad`.
    pub face_pref: String,
}

impl Stage {
    pub fn face(&self) -> Face {
        Face::resolve(&self.face_pref, self.form)
    }
    pub fn frame(&self) -> Frame {
        Frame {
            vw: self.vw,
            vh: self.vh,
            inset: self.inset,
            out_w: self.out_w,
            out_h: self.out_h,
            form: self.form,
            face: self.face(),
        }
    }
}

/// The page's events about the stage (`bridge.rs` routes them here).
pub fn on_event(ui: Ui, kind: &str, msg: &serde_json::Value) {
    let num = |k: &str| msg.get(k).and_then(|v| v.as_f64()).unwrap_or(0.0);
    let text = |k: &str| {
        msg.get(k)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string()
    };
    let mut stage = ui.live.stage;
    let mut next = stage();
    match kind {
        "stageView" => {
            next.vw = num("w");
            next.vh = num("h");
            next.inset = Insets {
                t: num("t"),
                r: num("r"),
                b: num("b"),
                l: num("l"),
            };
            next.form = Form::parse(&text("form"));
            next.out_w = num("vw");
            next.out_h = num("vh");
        }
        "arrangement" => next.cockpit = text("arr") == "cockpit",
        "consolePrefs" => next.face_pref = text("face"),
        _ => return,
    }
    if next != stage() {
        stage.set(next);
    }
}

/// Recompute the console's slots whenever what they depend on changes, and hand them to the
/// page. Also keeps the on-screen gamepad on exactly while the Gamepad face is chosen.
pub fn use_layout(ui: Ui) {
    use_effect(move || {
        let st = (ui.live.stage)();
        if st.vw <= 0.0 || st.vh <= 0.0 {
            return;
        }
        let l = layout::console(st.frame());
        let vars: serde_json::Map<String, serde_json::Value> = l
            .vars()
            .into_iter()
            .map(|(k, v)| (k, serde_json::Value::String(v)))
            .collect();
        let msg = serde_json::json!({
            "w": st.vw, "h": st.vh, "form": l.form.name(), "face": l.face.id(),
            "ws": l.ws.name(), "nav": l.nav.name(), "faces": l.faces.name(),
            "deck": if l.deck_col { "col" } else { "row" },
            "vars": vars,
        });
        bridge::call(format!("window.__wado.stageFit.apply({msg});"));
    });
    // A session opens in the arrangement this device asked for (`console { open }`), and the
    // page forgets it when the session ends.
    use_effect(move || {
        let on = (ui.live.session_on)();
        bridge::call(format!("window.__wado.stageMorph.session({on});"));
    });
    let mut last_face = use_signal(|| None::<Face>);
    use_effect(move || {
        let st = (ui.live.stage)();
        if st.vw <= 0.0 {
            return;
        }
        let face = st.face();
        if last_face() == Some(face) {
            return;
        }
        let was = last_face();
        last_face.set(Some(face));
        // The first reading only learns the face: a pad switched on in the control centre is
        // left alone until someone actually changes face.
        if was.is_none() && face != Face::Gamepad {
            return;
        }
        let on = face == Face::Gamepad;
        if (ui.set.pad_on)() != on {
            let mut pad_on = ui.set.pad_on;
            pad_on.set(on);
            crate::ui::gamepad::apply(ui);
        }
    });
}

/// Everything only the console has. Rendered while a session is on, in both arrangements.
pub fn render(ui: Ui) -> Element {
    if !(ui.live.session_on)() {
        return rsx! {};
    }
    let st = (ui.live.stage)();
    let face = st.face();
    rsx! {
        div { id: "surround", "data-only": "console", "aria-hidden": if st.cockpit { "true" } else { "false" },
            {top_band::render(ui)}
            {window_strip::render(ui)}
            {faces::switcher(ui, face)}
            {deck::render(ui, face)}
        }
    }
}

/// The console's ground and plates: the surfaces behind the controls. They never move, they fade
/// (spec §2.5). Rendered *before* the picture, so they paint under it.
pub fn backdrop(ui: Ui) -> Element {
    if !(ui.live.session_on)() {
        return rsx! {};
    }
    rsx! {
        div { class: "cground", "aria-hidden": "true" }
        div { class: "cplate bezel", "aria-hidden": "true" }
        div { class: "cplate deckplate", "aria-hidden": "true" }
    }
}
