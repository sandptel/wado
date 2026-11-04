//! `wado pair` run bare in a terminal asks first (`2026-11-04`): what the device that scans the
//! code may do — the defaults are `security { new-device }`, already ticked — and for how long, on
//! one screen (Enter alone takes the defaults). The code is made only after, so the decision rides
//! in it and nothing is decided once a device is already in.

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

/// The trust terms on offer, in the order ← → walks them.
const TERMS: &[(Option<u64>, &str)] = &[
    (None, "until you remove it"),
    (Some(3600), "1 hour"),
    (Some(86_400), "1 day"),
    (Some(7 * 86_400), "7 days"),
];

/// One screen, decided in place (`2026-11-04`, after "the interface is inhuman"): the defaults are
/// already ticked, so Enter alone shows the code; ↑↓ move, space switches, ←→ sets how long, Esc
/// leaves. Redrawn where it stands — no scrolling wall of questions. `None` when cancelled.
pub fn ask(defaults: &Grants) -> Option<(Grants, Option<u64>)> {
    use std::io::Write;

    use super::style::{accent, bad, bold, dim};
    use ratatui::crossterm::{
        cursor,
        event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
        queue,
        style::Print,
        terminal::{self, ClearType},
    };

    let mut g = defaults.clone();
    let mut row = 0usize;
    let mut term = 0usize;
    let mut out = std::io::stdout();
    let height = (ITEMS.len() + 5) as u16;
    let draw = |out: &mut std::io::Stdout, g: &Grants, row: usize, term: usize, first: bool| {
        if !first {
            let _ = queue!(
                out,
                cursor::MoveUp(height),
                cursor::MoveToColumn(0),
                terminal::Clear(ClearType::FromCursorDown)
            );
        }
        let mut lines = vec![format!(
            "  {}",
            bold("What may the device that scans this do?")
        )];
        for (i, (tok, label, note)) in ITEMS.iter().enumerate() {
            let on = has(g, tok);
            let mark = if on { accent("[x]") } else { dim("[ ]") };
            let here = if i == row { accent("›") } else { " ".into() };
            let label = if *tok == "devices" && on {
                bad(label)
            } else {
                label.to_string()
            };
            lines.push(format!("  {here} {mark} {label:<24} {}", dim(note)));
        }
        lines.push(String::new());
        lines.push(format!(
            "  {}   {} {} {}",
            bold("For"),
            dim("‹"),
            accent(TERMS[term].1),
            dim("›   ← →")
        ));
        lines.push(String::new());
        lines.push(format!(
            "  {}",
            dim("↑↓ move · space switch · enter show the code · esc cancel")
        ));
        for l in lines {
            let _ = queue!(out, Print(l), Print("\r\n"));
        }
        let _ = out.flush();
    };
    terminal::enable_raw_mode().ok()?;
    let _ = queue!(out, cursor::Hide);
    draw(&mut out, &g, row, term, true);
    let picked = loop {
        let Ok(Event::Key(k)) = event::read() else {
            continue;
        };
        if k.kind != KeyEventKind::Press {
            continue;
        }
        match k.code {
            KeyCode::Enter => break Some((g.clone(), TERMS[term].0)),
            KeyCode::Esc | KeyCode::Char('q') => break None,
            KeyCode::Char('c') if k.modifiers.contains(KeyModifiers::CONTROL) => break None,
            KeyCode::Up | KeyCode::Char('k') => row = row.checked_sub(1).unwrap_or(ITEMS.len() - 1),
            KeyCode::Down | KeyCode::Char('j') | KeyCode::Tab => row = (row + 1) % ITEMS.len(),
            KeyCode::Left | KeyCode::Char('h') => {
                term = term.checked_sub(1).unwrap_or(TERMS.len() - 1)
            }
            KeyCode::Right | KeyCode::Char('l') => term = (term + 1) % TERMS.len(),
            KeyCode::Char(' ') => {
                let tok = ITEMS[row].0;
                let on = !has(&g, tok);
                let _ = g.set(tok, on);
            }
            _ => continue,
        }
        draw(&mut out, &g, row, term, false);
    };
    let _ = queue!(out, cursor::Show, Print("\r\n"));
    let _ = out.flush();
    let _ = terminal::disable_raw_mode();
    picked
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
