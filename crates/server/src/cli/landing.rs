//! `wado` with no command: who this is, what is running, and the commands worth knowing.

use super::status::Status;
use super::style::{accent, bold, dim, dot, warn};

pub const VERSION: &str = concat!(
    env!("CARGO_PKG_VERSION"),
    " · ",
    env!("WADO_GIT"),
    " · built ",
    env!("WADO_BUILT")
);

/// `(group, [(usage, what it does)])`, in the order a newcomer needs them.
pub const COMMANDS: &[(&str, &[(&str, &str)])] = &[
    (
        "START",
        &[
            (
                "tui",
                "the control panel — starts the rig if nothing is running",
            ),
            (
                "daemon [addr]",
                "run a daemon in this terminal — prints the connect QR",
            ),
        ],
    ),
    (
        "DEVICES",
        &[
            (
                "pair [<grant>…]",
                "QR + link: whoever scans it is trusted, with these grants",
            ),
            ("devices", "the trusted devices, their access, who is on"),
            ("unpair <device>", "forget a device — by name or key prefix"),
            ("approve [once]", "let in the device waiting at the door"),
            (
                "allow|deny <device> <grant>…",
                "files-ro files-rw shells settings host",
            ),
        ],
    ),
    (
        "CONFIG",
        &[
            (
                "msg get|set|reload [--instance N]",
                "talk to a running daemon",
            ),
            (
                "validate [file]",
                "check config.kdl without starting anything",
            ),
        ],
    ),
];

pub fn print() {
    let s = super::status::read();
    println!();
    println!("  {}  {}", accent("▌wado"), dim(VERSION));
    println!(
        "  {}",
        dim("▌a headless Wayland desktop, streamed to your phone")
    );
    println!();
    running(&s);
    println!();
    let width = COMMANDS
        .iter()
        .flat_map(|(_, c)| c.iter().map(|(u, _)| u.len()))
        .max()
        .unwrap_or(0);
    for (group, cmds) in COMMANDS {
        println!("  {}", bold(group));
        for (usage, what) in *cmds {
            println!("    {} {usage:<width$}   {}", accent("wado"), dim(what));
        }
        println!();
    }
    println!(
        "  {}",
        dim(&format!(
            "config  {}",
            wado_config::paths::config_file().display()
        ))
    );
    println!();
}

fn running(s: &Status) {
    let id = s.remote_id.as_deref().unwrap_or("not yet chosen");
    if s.daemons.is_empty() {
        println!(
            "  {} {}   start one with {}",
            dot(false),
            bold("stopped"),
            accent("wado daemon")
        );
    } else {
        let pooled = s
            .pooled()
            .map_or(String::new(), |n| format!(" ({n} in the relay pool)"));
        println!(
            "  {} {}   {} daemon{}{pooled} · id {}",
            dot(true),
            bold("running"),
            s.daemons.len(),
            if s.daemons.len() == 1 { "" } else { "s" },
            bold(id)
        );
    }
    let relay = if s.relay.is_some() {
        format!("{} relay {}", dot(true), s.relay_addr)
    } else {
        format!("{} relay {} {}", dot(false), s.relay_addr, dim("(down)"))
    };
    let tunnel = s
        .tunnel
        .as_deref()
        .map_or(String::new(), |t| format!(" · tunnel {t}"));
    println!("             {relay}{tunnel}");
    let waiting = if s.pending > 0 {
        warn(&format!("{} waiting — wado approve", s.pending))
    } else {
        "0 waiting".into()
    };
    println!(
        "             {} session{} · {} trusted device{} · {waiting}",
        s.sessions.len(),
        if s.sessions.len() == 1 { "" } else { "s" },
        s.trusted,
        if s.trusted == 1 { "" } else { "s" },
    );
}

/// The command closest to a mistyped one, if any is close enough to be a guess.
pub fn suggest(typed: &str) -> Option<&'static str> {
    COMMANDS
        .iter()
        .flat_map(|(_, c)| c.iter())
        .map(|(u, _)| u.split(' ').next().unwrap_or(u))
        .map(|c| (distance(typed, c), c))
        .filter(|(d, c)| *d <= 2.max(c.len() / 3))
        .min_by_key(|(d, _)| *d)
        .map(|(_, c)| c)
}

/// Levenshtein, two rows.
fn distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut row = vec![i + 1];
        for (j, cb) in b.iter().enumerate() {
            row.push(
                (prev[j] + usize::from(ca != *cb))
                    .min(prev[j + 1] + 1)
                    .min(row[j] + 1),
            );
        }
        prev = row;
    }
    prev[b.len()]
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_typo_finds_its_command() {
        assert_eq!(super::suggest("devcies"), Some("devices"));
        assert_eq!(super::suggest("demon"), Some("daemon"));
        assert_eq!(super::suggest("xyzzy"), None);
    }
}
