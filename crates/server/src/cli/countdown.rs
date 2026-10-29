//! The live line under `wado pair`: how long the code has left, then what became of it — used,
//! expired, or revoked for too many wrong guesses.
//!
//! Only in a terminal. Piped, it prints the expiry once and returns, so a script gets its code
//! and moves on. Ctrl-C leaves the code live: it is already on screen, and may be typed yet.

use std::io::{IsTerminal, Write};
use std::time::Duration;

use super::style;
use crate::gate::{CodeState, Gate, Minted};

pub fn run(code: &Minted) {
    let gate = Gate::default();
    if !std::io::stdout().is_terminal() {
        println!("  {}", gate.state(code).say());
        return;
    }
    let mut out = std::io::stdout();
    loop {
        let state = gate.state(code);
        let line = match state {
            CodeState::Live(_) => style::dim(&state.say()),
            CodeState::Used => style::good(&state.say()),
            CodeState::Expired | CodeState::LockedOut => {
                style::warn(&format!("{} — run `wado pair` again", state.say()))
            }
        };
        // Clear to the end of the line: the next message can be shorter than this one.
        let _ = write!(out, "\r  {line}\x1b[K");
        let _ = out.flush();
        if !matches!(state, CodeState::Live(_)) {
            println!();
            return;
        }
        std::thread::sleep(Duration::from_secs(1));
    }
}
