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
    let cfg = wado_config::live::current();
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
        if let Some(line) = cfg.device.get(&d.key).and_then(input_line) {
            println!("    {:<width$}  {}", "", dim(&line));
        }
    }
    println!();
    println!(
        "  {}",
        dim(
            "wado allow|deny <device> files-ro files-rw shells settings host input clipboard · wado unpair <device>"
        )
    );
    println!();
    0
}

/// Its virtual input, when it has chosen anything: `gamepad racing · a controller · 2 saved ·
/// pointer 1.25× · scroll 0.5×`. `None` for a device that never opened the gamepad or changed its speed.
fn input_line(d: &wado_config::schema::Device) -> Option<String> {
    let mut parts = Vec::new();
    if let Some(g) = &d.gamepad {
        let drives = if g.drives == "keys" {
            "keys & mouse"
        } else {
            "a controller"
        };
        let mut s = format!("gamepad {} · {drives}", g.layout);
        if !g.mine.is_empty() {
            s.push_str(&format!(" · {} saved", g.mine.len()));
        }
        parts.push(s);
    }
    if let Some(t) = &d.trackpad {
        if t.speed != 1.0 {
            parts.push(format!("pointer {}×", t.speed));
        }
        if t.scroll_speed != 1.0 {
            parts.push(format!("scroll {}×", t.scroll_speed));
        }
    }
    (!parts.is_empty()).then(|| parts.join(" · "))
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
        "{files}   {}  {}  {}  {}  {}  {}  {}",
        flag(g.shells, "shells"),
        flag(g.settings, "settings"),
        flag(g.host, "host"),
        flag(g.input, "input"),
        flag(g.clipboard, "clipboard"),
        flag(g.join, "join"),
        flag(g.takeover, "takeover")
    )
}

pub fn unpair(args: &[String]) -> i32 {
    let Some(who) = args.first() else {
        eprintln!("usage: wado unpair <device>   — its name, or the start of its key");
        return 2;
    };
    let gate = Gate::default();
    let r = gate
        .unpair(who)
        .map(|name| format!("unpaired {} — it must be paired again", bold(&name)));
    report(r).max(pick_help(&gate, who, "unpair", ""))
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
    let gate = Gate::default();
    let r = gate
        .allow(who, &tokens, on)
        .map(|(name, g)| format!("{} may now: {}", bold(&name), describe(&g)));
    report(r).max(pick_help(&gate, who, verb, &tokens.join(" ")))
}

/// After a lookup that named no device: which devices there are, and the command for the first,
/// ready to paste. Returns 0 — the failure itself was already reported.
fn pick_help(gate: &Gate, who: &str, verb: &str, rest: &str) -> i32 {
    let devices = gate.devices();
    if gate.find(who).is_ok() || devices.is_empty() {
        return 0;
    }
    eprintln!("  your devices:");
    for d in &devices {
        eprintln!("    {}", Gate::typed(&d.name));
    }
    let example = format!("wado {verb} {} {rest}", Gate::typed(&devices[0].name));
    eprintln!("  e.g. {}", accent(example.trim_end()));
    eprintln!(
        "  {}",
        dim("the start of a name works too, any case: wado allow android input")
    );
    0
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_input_line_says_the_gamepad_and_a_changed_speed() {
        let c = wado_config::kdl::parse(
            r#"device "k" { gamepad { layout "racing"; mine { mine-1 "A" "racing" "{}"; mine-2 "B" "xbox" "{}" } }
               trackpad { speed 1.25 } }
               device "j" { trackpad { speed 1 } }
               device "m" { gamepad { drives "keys" } }
               device "s" { trackpad { scroll-speed 0.5 } }"#,
        )
        .unwrap();
        assert_eq!(
            input_line(&c.device["k"]).as_deref(),
            Some("gamepad racing · a controller · 2 saved · pointer 1.25×")
        );
        assert_eq!(input_line(&c.device["j"]), None, "nothing chosen");
        assert_eq!(input_line(&c.device["s"]).as_deref(), Some("scroll 0.5×"));
        assert_eq!(
            input_line(&c.device["m"]).as_deref(),
            Some("gamepad xbox · keys & mouse")
        );
    }
}
