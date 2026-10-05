//! `wado tui` — the control panel for the person running this computer: the rig, the sessions,
//! the devices and what each may do, pairing, and the security switches. Everything it changes
//! lands in the same files the daemons read, so it and the phones' settings never disagree.
//!
//! - [`app`] — state and keys.
//! - [`data`] — one read of the world.
//! - [`actions`] — the changes, and where they are written.
//! - [`view`] — drawing.
//! - [`theme`] — colours and glyphs.
//! - [`rig`] — starting the rig when nothing is running.

mod actions;
mod app;
mod data;
pub mod rig;
mod theme;
mod view;

use std::time::Duration;

use ratatui::crossterm::event::{self, Event, KeyEventKind};

pub fn run(args: &[String]) -> i32 {
    if args.first().map(String::as_str) == Some("--frame") {
        let panel = args.get(2).and_then(|p| p.parse::<usize>().ok());
        return frame(args.get(1).map_or("140x44", String::as_str), panel);
    }
    if let Err(e) = rig::ensure() {
        eprintln!("  {e}");
        return 1;
    }
    let mut term = ratatui::init();
    let mut app = app::App::new();
    let res = (|| -> std::io::Result<()> {
        while !app.quit {
            term.draw(|f| view::draw(f, &app))?;
            if event::poll(Duration::from_millis(250))? {
                if let Event::Key(k) = event::read()? {
                    if k.kind == KeyEventKind::Press {
                        app.key(k);
                    }
                }
            }
            app.tick();
        }
        Ok(())
    })();
    ratatui::restore();
    match res {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("  {e}");
            1
        }
    }
}

/// `wado tui --frame [WxH] [panel 1-5]`: one frame as plain text, without touching the terminal or starting
/// anything — to see the panel from a script, a log, or a session with no terminal of its own.
fn frame(size: &str, panel: Option<usize>) -> i32 {
    let (w, h) = size
        .split_once('x')
        .and_then(|(w, h)| Some((w.parse().ok()?, h.parse().ok()?)))
        .unwrap_or((140, 44));
    let mut term = match ratatui::Terminal::new(ratatui::backend::TestBackend::new(w, h)) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("{e}");
            return 1;
        }
    };
    let mut app = app::App::new();
    if let Some(p) = panel.filter(|p| (1..=5).contains(p)) {
        app.focus = app::Panel::ALL[p - 1];
    }
    if let Err(e) = term.draw(|f| view::draw(f, &app)) {
        eprintln!("{e}");
        return 1;
    }
    let buf = term.backend().buffer();
    for y in 0..h {
        let line: String = (0..w).map(|x| buf[(x, y)].symbol()).collect();
        println!("{}", line.trim_end());
    }
    0
}
