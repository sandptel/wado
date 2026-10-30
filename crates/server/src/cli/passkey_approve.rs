//! `wado passkey approve` — answer a phone that asked to be let in without its passkey
//! (`crate::passkey::approve`). Only the person at this computer can: that is the point.
//!
//! ```text
//!   wado passkey approve                    each waiting request, y or n
//!   wado passkey approve <device> --yes     answer that one without asking (also --no)
//!   wado passkey approve --wait             wait up to 2 minutes for a request to come in
//! ```
//!
//! The answer is read from stdin, a line per request, so a script can pipe it (the e2e does).

use std::io::{BufRead, IsTerminal, Write};
use std::time::{Duration, Instant};

use super::style::{accent, bold, dim, good, warn};
use crate::passkey::approve::{Approvals, Ask, WAIT};

pub fn run(args: &[String]) -> i32 {
    let flag = |f: &str| args.iter().any(|a| a == f);
    let who = args.iter().find(|a| !a.starts_with("--")).cloned();
    let preset = match (flag("--yes"), flag("--no")) {
        (true, false) => Some(true),
        (false, true) => Some(false),
        (false, false) => None,
        (true, true) => {
            eprintln!("  --yes or --no, not both");
            return 2;
        }
    };
    let approvals = Approvals::default();
    let mine = |a: &Ask| {
        who.as_deref().is_none_or(|w| {
            a.device == w || a.key.starts_with(w) || a.device.to_lowercase() == w.to_lowercase()
        })
    };
    let wait = flag("--wait");
    let since = Instant::now();
    let mut asks: Vec<Ask> = approvals.asking().into_iter().filter(|a| mine(a)).collect();
    if asks.is_empty() && wait {
        println!(
            "  {}",
            dim(
                "waiting for a phone to ask — tap \"Approve on the computer\" on it (Ctrl-C to stop)"
            )
        );
        while asks.is_empty() && since.elapsed() < WAIT {
            std::thread::sleep(Duration::from_millis(300));
            asks = approvals.asking().into_iter().filter(|a| mine(a)).collect();
        }
    }
    if asks.is_empty() {
        println!("  nothing is waiting for approval");
        return 0;
    }
    let mut lines = std::io::stdin().lock().lines();
    let tty = std::io::stdin().is_terminal();
    let mut code = 0;
    for a in asks {
        println!();
        println!(
            "  {} asks to unlock without its passkey ({} s left).",
            bold(&a.device),
            a.left()
        );
        println!(
            "  {}",
            dim("Say yes only if you — or someone with you — just tapped it on that phone.")
        );
        let yes = match preset {
            Some(y) => y,
            None => loop {
                print!("  approve? y — let it in · n — turn it away: ");
                let _ = std::io::stdout().flush();
                match lines.next() {
                    Some(Ok(l)) => match l.trim().to_lowercase().as_str() {
                        "y" | "yes" => break true,
                        "n" | "no" => break false,
                        _ if !tty => break false,
                        _ => continue,
                    },
                    // No more input: nobody said yes.
                    _ => break false,
                }
            },
        };
        if approvals.answer(&a.id, yes) {
            if yes {
                println!(
                    "  {} — {} unlocks as with its passkey",
                    good("approved"),
                    bold(&a.device)
                );
            } else {
                println!(
                    "  {} — {} stays locked",
                    warn("turned away"),
                    bold(&a.device)
                );
            }
        } else {
            println!("  {} is no longer waiting", bold(&a.device));
            code = 1;
        }
    }
    if preset.is_none() {
        println!();
        println!(
            "  {}",
            dim(&format!(
                "a phone whose passkey never works can be asked for none: {}",
                accent("wado passkey require <device> off")
            ))
        );
    }
    code
}
