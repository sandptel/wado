//! `wado tui` — the control panel for the person running this computer: the rig, the sessions,
//! the devices and what each may do, pairing, and the security switches. Everything it changes
//! lands in the same files the daemons read, so it and the phones' settings never disagree.
//!
//! - [`app`] — state and keys.
//! - [`data`] — one read of the world.
//! - [`actions`] — the changes, and where they are written.
//! - [`view`] — drawing.
//! - [`load`] — each daemon's CPU and memory.
//! - [`theme`] — colours and glyphs.
//! - [`rig`] — starting a daemon when none is running.

mod actions;
mod app;
mod data;
mod load;
pub mod rig;
mod theme;
mod view;

use std::time::Duration;

use ratatui::crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyEventKind},
    execute,
};

pub fn run(args: &[String]) -> i32 {
    if args.first().map(String::as_str) == Some("--frame") {
        let card = args.get(2).and_then(|p| p.parse::<usize>().ok());
        return frame(args.get(1).map_or("150x46", String::as_str), card);
    }
    if let Err(e) = rig::ensure() {
        eprintln!("  {e}");
        return 1;
    }
    let mut term = ratatui::init();
    let _ = execute!(std::io::stdout(), EnableMouseCapture);
    let mut app = app::App::new();
    let res = (|| -> std::io::Result<()> {
        while !app.quit {
            term.draw(|f| view::draw(f, &app))?;
            if event::poll(Duration::from_millis(250))? {
                match event::read()? {
                    Event::Key(k) if k.kind == KeyEventKind::Press => app.key(k),
                    Event::Mouse(m) => app.mouse(m),
                    _ => {}
                }
            }
            app.tick();
        }
        Ok(())
    })();
    // Withdraw the code on screen before the terminal is handed back.
    drop(app);
    let _ = execute!(std::io::stdout(), DisableMouseCapture);
    ratatui::restore();
    match res {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("  {e}");
            1
        }
    }
}

/// `wado tui --frame [WxH] [card 1-3]`: one frame as plain text, without touching the terminal or starting
/// anything — to see the panel from a script, a log, or a session with no terminal of its own.
fn frame(size: &str, card: Option<usize>) -> i32 {
    let (w, h) = size
        .split_once('x')
        .and_then(|(w, h)| Some((w.parse().ok()?, h.parse().ok()?)))
        .unwrap_or((150, 46));
    let mut term = match ratatui::Terminal::new(ratatui::backend::TestBackend::new(w, h)) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("{e}");
            return 1;
        }
    };
    let mut app = app::App::new();
    if let Some(c) = card.filter(|c| (1..=3).contains(c)) {
        app.focus = app::Card::ALL[c - 1];
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
