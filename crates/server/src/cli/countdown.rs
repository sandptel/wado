//! The live part of `wado pair`: how long the code has left, the check words when a device types
//! it in — answered here, y or n — and then how it ended: paired, refused, expired, or revoked
//! for too many wrong guesses.
//!
//! In a terminal the countdown redraws one line. Piped, it prints the expiry once and returns,
//! so a script gets its code and moves on — unless `--wait`, which prints each change as a line
//! and reads the answer from stdin (what the e2e scripts drive). Ctrl-C leaves the code live: it
//! is already on screen, and may be typed yet.

use std::io::{BufRead, IsTerminal, Write};
use std::sync::mpsc;
use std::time::Duration;

use super::style;
use crate::gate::{CodeState, Gate, Minted};

/// Lines typed on stdin, read on a thread so the countdown keeps going meanwhile.
fn stdin_lines() -> mpsc::Receiver<String> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for line in std::io::stdin().lock().lines() {
            let Ok(line) = line else { return };
            if tx.send(line).is_err() {
                return;
            }
        }
    });
    rx
}

pub fn run(code: &Minted, wait: bool) {
    let gate = Gate::default();
    let tty = std::io::stdout().is_terminal();
    if !tty && !wait {
        println!("  {}", gate.state(code).say());
        return;
    }
    let mut out = std::io::stdout();
    let mut answers: Option<mpsc::Receiver<String>> = None;
    let mut said_live = false;
    loop {
        let state = gate.state(code);
        match &state {
            CodeState::Live(_) if tty => {
                // Clear to the end of the line: the next message can be shorter than this one.
                let _ = write!(out, "\r  {}\x1b[K", style::dim(&state.say()));
            }
            CodeState::Live(_) => {
                if !said_live {
                    println!("  {}", state.say());
                    said_live = true;
                }
            }
            CodeState::Confirm { words, device, .. } => {
                let rx = answers.get_or_insert_with(|| {
                    if tty {
                        let _ = write!(out, "\r\x1b[K");
                    }
                    println!("  {device} typed the code. Does it show these four words?");
                    println!("      {}", style::accent(words));
                    let _ = write!(out, "  y — they match · n — they don't: ");
                    stdin_lines()
                });
                if let Ok(a) = rx.try_recv() {
                    match a.trim().to_lowercase().as_str() {
                        "y" | "yes" => {
                            gate.answer(&code.typed, true);
                        }
                        "n" | "no" => {
                            gate.answer(&code.typed, false);
                        }
                        _ => {
                            let _ = write!(out, "  y or n: ");
                        }
                    }
                }
            }
            CodeState::Used => {
                end(tty, &style::good(&state.say()));
                return;
            }
            CodeState::Expired | CodeState::LockedOut | CodeState::Refused(_) => {
                end(
                    tty,
                    &style::warn(&format!("{} — run `wado pair` again", state.say())),
                );
                return;
            }
        }
        let _ = out.flush();
        std::thread::sleep(Duration::from_millis(250));
    }
}

fn end(tty: bool, line: &str) {
    if tty {
        print!("\r\x1b[K");
    }
    println!("  {line}");
}
