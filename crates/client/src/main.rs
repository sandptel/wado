//! wado web client — a Dioxus (WASM) app that runs in the browser.
//!
//! A landing page to pick a host and start, a live WebRTC video stage, and a control centre
//! sheet for everything else.
//!
//! The split, and why: UI state, rendering and config-building are native Dioxus; everything
//! that can only happen in a browser — fetch, WebRTC, `<video>.srcObject`, SSE, storage,
//! theming — lives in `js/` and is driven through [`bridge`]. This file only assembles.

mod actions;
mod bridge;
mod cfg;
mod debug;
mod persist;
mod profile;
mod res;
mod state;
mod theme;
mod ui;

use dioxus::prelude::*;

use state::{Live, Settings, Ui};

fn main() {
    console_error_panic_hook::set_once();
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    let ui = Ui {
        set: Settings::new(),
        live: Live::new(),
    };

    bridge::run(ui);

    // Persist on any change. Reading every field is what subscribes this effect to all of
    // them, so `snapshot` doubles as the dependency list and cannot drift out of date.
    //
    // The `loaded` gate is load-bearing, not defensive: effects run before the bridge's async
    // load finishes, so without it the first run would write the defaults over the saved blob
    // and nothing would ever persist. Returning early also means this run subscribes only to
    // `loaded`; the full subscription is taken on the re-run once loading is done.
    use_effect(move || {
        if !(ui.live.loaded)() {
            return;
        }
        bridge::save(&persist::snapshot(ui));
    });

    // A saved resolution must not outlive the options that offer it. The device-exact list
    // is only known once the bridge reports the screen, which can land either side of the
    // saved blob — and a `res` matching no `<option>` renders as a blank select that starts
    // a session at whatever the parse fallback is. So once both are in, a value that is not
    // on offer is replaced by the device's own default rather than silently kept.
    use_effect(move || {
        let (w, h) = ((ui.live.screen_w)(), (ui.live.screen_h)());
        if w == 0 || h == 0 || !(ui.live.loaded)() {
            return;
        }
        let current = (ui.set.res)();
        let device = res::options(w, h, (ui.live.screen_phone)());
        let exclude: Vec<String> = device.iter().map(|(v, _)| v.clone()).collect();
        let offered = device
            .iter()
            .map(|(v, _)| v.clone())
            .chain(
                res::catalog::options(w, h, &exclude)
                    .into_iter()
                    .map(|(v, _)| v),
            )
            .chain(["custom".into()]);
        if !offered.into_iter().any(|v| v == current) {
            if let Some(d) = res::default_value(w, h) {
                ui.set.res.clone().set(d);
            }
        }

        // Same gate, same reason: pixel density is only known once the bridge reports it, and
        // a scale chosen before then is a guess. Only an untouched default is replaced — an
        // explicit choice, including a deliberate 1x, is left alone.
        if (ui.set.scale)() == crate::state::SCALE_UNSET {
            ui.set
                .scale
                .clone()
                .set(res::default_scale((ui.live.screen_dpr)()).to_string());
        }
    });

    // Keep the log panel pinned to the newest line, unless the user has scrolled up to read
    // history — following the tail while someone is reading is worse than not following it.
    use_effect(move || {
        let _ = ui.live.logs.read().len();
        bridge::call(
            "var w=document.getElementById('wado-logwrap'); \
             if(w){var atBottom=w.scrollHeight-w.scrollTop-w.clientHeight<40; \
             if(atBottom) w.scrollTop=w.scrollHeight;}"
                .to_string(),
        );
    });

    rsx! {
        document::Stylesheet { href: asset!("/assets/base.css") }
        document::Stylesheet { href: asset!("/assets/layout.css") }
        document::Stylesheet { href: asset!("/assets/cc.css") }
        document::Stylesheet { href: asset!("/assets/landing.css") }
        document::Stylesheet { href: asset!("/assets/stage.css") }
        document::Stylesheet { href: asset!("/assets/gamepad.css") }

        // The stage is the whole screen, always. Everything else floats over it and never
        // takes space from the picture — the control centre included (Decision Log 2026-10-02).
        div { class: if (ui.live.cc_open)() { "app cc-open" } else { "app" },
            main { id: "stage",
                {ui::stage::render(ui)}
                {ui::dock::render(ui)}
            }
            {ui::landing::render(ui)}
            {ui::toast::render(ui)}
            {ui::cc::render(ui)}
        }
    }
}
