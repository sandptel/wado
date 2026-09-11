//! wado web client — a Dioxus (WASM) app that runs in the browser.
//!
//! A config panel (connection / session / live / appearance / debug), Start-Stop, a live
//! WebRTC video stage, and a log panel fed by the server's `/events` SSE stream.
//!
//! The split, and why: UI state, rendering and config-building are native Dioxus; everything
//! that can only happen in a browser — fetch, WebRTC, `<video>.srcObject`, SSE, storage,
//! theming — lives in `js/` and is driven through [`bridge`]. This file only assembles.

mod actions;
mod bridge;
mod cfg;
mod debug;
mod persist;
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
        document::Stylesheet { href: asset!("/assets/stage.css") }

        // `sheet` on the shell is what the layout breakpoint reads to decide whether the
        // panel is docked beside the video or slid over it. One markup tree, two
        // presentations — the alternative is two panels to keep in sync.
        div { class: "app", "data-sheet": if (ui.live.sheet_open)() { "open" } else { "shut" },
            // Dismiss-on-tap-away. Present only below the breakpoint (CSS), where the
            // panel covers the video and the bar's toggle is underneath it.
            div {
                id: "scrim",
                onclick: move |_| {
                    let mut live = ui.live;
                    live.sheet_open.set(false);
                },
            }
            aside { id: "panel", {ui::panel(ui)} }
            main { id: "stage",
                {ui::stage::render(ui)}
                {ui::bar::render(ui)}
            }
        }
    }
}
