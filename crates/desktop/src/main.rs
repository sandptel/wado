// SPDX-License-Identifier: AGPL-3.0-only
//! `wado-desktop` — one app, two modes.
//!
//! - **This computer** (host console): the daemons, devices and grants, pairing with QR and
//!   check words, sessions, live stats, logs, and every section of config.kdl.
//! - **Other computers** (client): the computers this app knows, whether each is up, and each
//!   one's page with its doors.
//!
//! Layout of the crate, one job per module:
//!
//! - `ui/` — the Slint UI (compiled at build time by `build.rs`).
//! - [`caps`] — the only code that calls the daemon's library or the host; shaped after the
//!   platform layer so it can switch to `wado-platform`.
//! - [`bridge`] — pushed state (`watch` channels) into the event loop.
//! - [`host`], [`client`] — each mode's UI state and actions.
//! - [`app`], [`theme`], [`prefs`], [`toast`], [`models`] — the window, base16, appearance
//!   choices, the one-line toast, model upkeep.
//! - [`shot`] — `--screenshot`: every screen rendered headless with the software renderer.
//!
//! ```text
//! wado-desktop [--host | --client] [--scheme NAME] [--fixture]
//! wado-desktop --screenshot DIR [--size WxH] [--scheme NAME] [--fixture]
//! ```

slint::include_modules!();

mod app;
mod bridge;
mod caps;
mod client;
mod host;
mod models;
mod prefs;
mod shot;
mod theme;
mod toast;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let flag = |f: &str| args.iter().any(|a| a == f);
    let value = |f: &str| {
        args.iter()
            .position(|a| a == f)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    if flag("--help") || flag("-h") {
        println!(
            "wado-desktop [--host | --client] [--scheme NAME] [--fixture]\n\
             wado-desktop --screenshot DIR [--size WxH] [--scheme NAME] [--fixture]"
        );
        return;
    }
    let mode = if flag("--host") {
        Some(0)
    } else if flag("--client") {
        Some(1)
    } else {
        None
    };
    let opts = app::Options {
        mode,
        scheme: value("--scheme"),
        live: true,
    };
    if let Some(dir) = value("--screenshot") {
        let size = value("--size").unwrap_or_else(|| "1280x820".into());
        std::process::exit(shot::run(&dir, &size, opts, flag("--fixture")));
    }

    // A made-up computer instead of this one (`caps::fixture`) — for demos and for looking at a
    // populated console without touching the real trust list.
    let _fx = if flag("--fixture") {
        let root =
            std::env::temp_dir().join(format!("wado-desktop-fixture-{}", std::process::id()));
        match caps::fixture::install(&root) {
            Ok(f) => Some(f),
            Err(e) => {
                eprintln!("fixture: {e}");
                std::process::exit(1);
            }
        }
    } else {
        None
    };
    let ui = match AppWindow::new() {
        Ok(ui) => ui,
        Err(e) => {
            eprintln!("wado-desktop: could not open a window: {e}");
            std::process::exit(1);
        }
    };
    app::setup(&ui, &opts);
    host::install(&ui, true);
    client::install(&ui, true);
    if let Err(e) = ui.run() {
        eprintln!("wado-desktop: {e}");
        std::process::exit(1);
    }
}
