//! `wado pair` run bare in a terminal asks first (`2026-11-04`): what the device that scans the
//! code may do — the defaults are `security { new-device }` — and for how long. The code is made
//! only after, so the decision rides in it and nothing is decided once a device is already in.

use std::io::{BufRead, Write};

use crate::gate::Grants;

/// The checklist, in the words a person reads: (token, label, note).
const ITEMS: &[(&str, &str, &str)] = &[
    ("clipboard", "Clipboard", "what you copy, both ways"),
    (
        "files-ro",
        "Files, read only",
        "your home folder, minus ~/.config/wado",
    ),
    ("files-rw", "Files, read and write", ""),
    (
        "input",
        "Trackpad and typing",
        "mouse and keys on this computer itself",
    ),
    ("host", "Sound, Bluetooth, sleep", ""),
    (
        "join",
        "Join a session in use",
        "sees what is on that screen",
    ),
    ("takeover", "Take over a session", ""),
    ("shells", "Shells", "a terminal on this computer"),
    (
        "settings",
        "Settings",
        "what can run code or cut the computer off",
    ),
    (
        "devices",
        "Manage devices",
        "it may let other devices in, for up to 7 days — only ever given here",
    ),
];

/// Seconds for `1h`, `8h`, `1d`, `7d`, `30m`; `None` for `forever`. A bad one is an error.
pub fn parse_for(s: &str) -> Result<Option<u64>, String> {
    let s = s.trim().to_lowercase();
    if matches!(s.as_str(), "forever" | "always" | "-" | "") {
        return Ok(None);
    }
    let (n, unit) = s.split_at(s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len()));
    let n: u64 = n
        .parse()
        .map_err(|_| format!("`{s}` is not a time — use 30m, 8h, 1d, 7d or forever"))?;
    let k = match unit {
        "m" | "min" => 60,
        "h" => 3600,
        "d" => 86_400,
        _ => {
            return Err(format!(
                "`{s}` is not a time — use 30m, 8h, 1d, 7d or forever"
            ));
        }
    };
    if n == 0 {
        // Zero is not "forever": a typo must never widen trust.
        return Err("a time must be more than zero — or say forever".into());
    }
    Ok(Some(n * k))
}

fn has(g: &Grants, token: &str) -> bool {
    g.tokens().split(',').any(|t| t == token)
}

/// Ask on the terminal. `None` when stdin closed.
pub fn ask(defaults: &Grants) -> Option<(Grants, Option<u64>)> {
    let mut g = defaults.clone();
    let stdin = std::io::stdin();
    let mut line = String::new();
    loop {
        println!("\n  What can the device that scans this do?  The desktop is always included.");
        for (i, (tok, label, note)) in ITEMS.iter().enumerate() {
            let mark = if has(&g, tok) { "[x]" } else { "[ ]" };
            let note = if note.is_empty() {
                String::new()
            } else {
                format!("  — {note}")
            };
            println!("   {} {mark} {label}{note}", i + 1);
        }
        print!("  Numbers to switch (e.g. 1 3), Enter to go on: ");
        std::io::stdout().flush().ok();
        line.clear();
        if stdin.lock().read_line(&mut line).ok()? == 0 {
            return None;
        }
        let picks: Vec<usize> = line
            .split_whitespace()
            .filter_map(|w| w.parse().ok())
            .collect();
        if picks.is_empty() {
            break;
        }
        for n in picks {
            if let Some((tok, ..)) = ITEMS.get(n.wrapping_sub(1)) {
                let on = !has(&g, tok);
                let _ = g.set(tok, on);
            }
        }
    }
    let choices = [
        ("1 hour", Some(3600)),
        ("1 day", Some(86_400)),
        ("7 days", Some(7 * 86_400)),
        ("until you remove it", None),
    ];
    println!("\n  For how long?");
    for (i, (w, _)) in choices.iter().enumerate() {
        println!("   {} {w}{}", i + 1, if i == 3 { "  (Enter)" } else { "" });
    }
    print!("  ");
    std::io::stdout().flush().ok();
    line.clear();
    if stdin.lock().read_line(&mut line).ok()? == 0 {
        return None;
    }
    let pick = line
        .trim()
        .parse::<usize>()
        .ok()
        .and_then(|n| choices.get(n.wrapping_sub(1)))
        .map_or(None, |c| c.1);
    println!();
    Some((g, pick))
}

#[cfg(test)]
mod tests {
    #[test]
    fn times_read_or_are_refused() {
        assert_eq!(super::parse_for("7d"), Ok(Some(7 * 86_400)));
        assert_eq!(super::parse_for("90m"), Ok(Some(5400)));
        assert_eq!(super::parse_for("forever"), Ok(None));
        assert!(super::parse_for("soon").is_err());
        assert!(super::parse_for("3w").is_err());
        assert!(
            super::parse_for("0d").is_err(),
            "zero is a typo, not forever"
        );
    }
}
