//! `wado devices` — the trusted devices and what each may do; `wado unpair`, `wado allow`,
//! `wado deny` — change that.

use super::style::{accent, bad, bold, dim, dot, good};
use crate::gate::{Gate, Grants, grants::TOKENS};

pub fn list() -> i32 {
    let gate = Gate::default();
    let devices = gate.devices();
    if devices.is_empty() {
        println!("  no trusted devices yet — {}", accent("wado pair"));
        return 0;
    }
    let on: Vec<String> = crate::sessions::list()
        .into_iter()
        .filter_map(|s| s.viewer)
        .collect();
    let width = devices
        .iter()
        .map(|d| d.name.chars().count())
        .max()
        .unwrap_or(0);
    println!();
    for d in &devices {
        let pin = if d.pinned {
            good("📌 QR      ")
        } else {
            dim("first use ")
        };
        let owner = crate::config::link::is_owner(&d.key, &gate);
        let role = if owner {
            accent("owner ")
        } else {
            "      ".into()
        };
        println!(
            "  {} {:<width$}  {}  {pin} {role} {}",
            dot(on.contains(&d.name)),
            d.name,
            dim(&d.key[..d.key.len().min(8)]),
            badges(&gate.grants(&d.key), d.pinned),
        );
    }
    println!();
    println!(
        "  {}",
        dim(
            "wado allow|deny <device> files-ro files-rw shells settings host · wado unpair <device>"
        )
    );
    println!();
    0
}

/// `files rw · shells · host`, dim where a grant is off.
fn badges(g: &Grants, pinned: bool) -> String {
    let files = match (g.files, pinned) {
        ("none", _) => dim("files –"),
        (l, true) => format!("files {}", bold(l)),
        (l, false) => dim(&format!("files {l} (needs QR)")),
    };
    // A mark as well as a colour: piped or with NO_COLOR, dim is not there to tell them apart.
    let flag = |on: bool, t: &str| {
        if on {
            format!("{} {t}", good("✓"))
        } else {
            dim(&format!("· {t}"))
        }
    };
    format!(
        "{files}   {}  {}  {}  {}",
        flag(g.shells, "shells"),
        flag(g.settings, "settings"),
        flag(g.host, "host"),
        flag(g.clipboard, "clipboard")
    )
}

pub fn unpair(args: &[String]) -> i32 {
    let Some(who) = args.first() else {
        eprintln!("usage: wado unpair <device>   — its name, or the start of its key");
        return 2;
    };
    report(Gate::default().unpair(who).map(|name| {
        format!(
            "unpaired {} — it must be approved or paired again",
            bold(&name)
        )
    }))
}

/// `wado allow <device> <grant>…`, or `deny` with `on` false.
pub fn allow(args: &[String], on: bool) -> i32 {
    let verb = if on { "allow" } else { "deny" };
    let Some((who, tokens)) = args.split_first().filter(|(_, t)| !t.is_empty()) else {
        eprintln!(
            "usage: wado {verb} <device> <grant>…   grants: {}",
            TOKENS.join(" ")
        );
        return 2;
    };
    let tokens: Vec<&str> = tokens.iter().map(String::as_str).collect();
    report(
        Gate::default()
            .allow(who, &tokens, on)
            .map(|(name, g)| format!("{} may now: {}", bold(&name), describe(&g))),
    )
}

/// Grants as tokens, or what having none means.
pub fn describe(g: &Grants) -> String {
    let t = g.tokens();
    if t.is_empty() {
        "the desktop only".into()
    } else {
        t
    }
}

fn report(r: Result<String, String>) -> i32 {
    match r {
        Ok(msg) => {
            println!("  {msg}");
            0
        }
        Err(e) => {
            eprintln!("  {}", bad(&e));
            1
        }
    }
}
