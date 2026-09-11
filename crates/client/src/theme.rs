//! base16 theming: parsing a pasted scheme, and handing the result to the bridge.
//!
//! The palette itself lives in `js/theme.js` — it has to, so the saved scheme can be applied
//! before Rust renders and the page never flashes the wrong colours. What lives here is the
//! part that is pure data: turning whatever the user pasted into 16 values.
//!
//! base16 schemes are published as YAML (`base00: "1d2021"`, sometimes nested under
//! `palette:`, sometimes with a `#`). Rather than take a YAML dependency to read sixteen hex
//! numbers, this scans for the keys directly and falls back to "sixteen hex triples in order"
//! so a bare list pasted from anywhere also works.

use dioxus::prelude::*;

/// Bundled scheme names, in picker order. Must match the keys in `js/theme.js`.
pub const SCHEMES: &[&str] = &["default-dark", "gruvbox-dark", "nord", "tomorrow-night"];

fn is_hex(b: u8) -> bool {
    b.is_ascii_hexdigit()
}

/// The first run of *exactly* six hex digits at or after `from`, with the offset just past
/// it so a caller can keep scanning without having to search for its own result again.
///
/// Length is checked exactly so a longer hex-ish run (a hash, an id) is skipped rather than
/// truncated into a plausible-looking colour.
fn hex6_after(hay: &str, from: usize) -> Option<(String, usize)> {
    let b = hay.as_bytes();
    let mut i = from;
    while i < b.len() {
        if !is_hex(b[i]) {
            i += 1;
            continue;
        }
        let start = i;
        while i < b.len() && is_hex(b[i]) {
            i += 1;
        }
        if i - start == 6 {
            return Some((hay[start..i].to_ascii_lowercase(), i));
        }
    }
    None
}

/// Parse a pasted base16 scheme into 16 lowercase hex triples, base00 first.
///
/// Returns `None` unless all sixteen are found — a half-applied palette is worse than an
/// unchanged one, so partial input is rejected rather than filled in.
pub fn parse(text: &str) -> Option<Vec<String>> {
    let lower = text.to_ascii_lowercase();

    // Keyed form: find each `base0N` and take the next hex6 after it. Searching forward from
    // the key is what keeps a scheme's name or author line from being read as a colour.
    let keyed: Option<Vec<String>> = (0..16)
        .map(|i| {
            let key = format!("base{:02x}", i);
            let at = lower.find(&key)? + key.len();
            hex6_after(&lower, at).map(|(h, _)| h)
        })
        .collect();
    if let Some(v) = keyed {
        return Some(v);
    }

    // Bare form: exactly sixteen hex triples, in order. Anything else is ambiguous.
    let mut all = Vec::new();
    let mut at = 0;
    while let Some((h, end)) = hex6_after(&lower, at) {
        all.push(h);
        at = end;
        if all.len() > 16 {
            return None;
        }
    }
    (all.len() == 16).then_some(all)
}

/// Push the current selection to the bridge. `custom` wins when it parses, which is what lets
/// a pasted scheme survive a reload without ever being added to the bundled list.
pub fn apply(name: &str, custom: &str) {
    let parsed = parse(custom);
    let name = name.to_string();
    spawn(async move {
        let args = format!(
            "{}, {}",
            serde_json::to_string(&name).unwrap_or_else(|_| "null".into()),
            serde_json::to_string(&parsed).unwrap_or_else(|_| "null".into()),
        );
        let _ = document::eval(&format!("window.__wado.setTheme({args});")).await;
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    const YAML: &str = r#"
scheme: "Gruvbox dark, hard"
author: "Dawid Kurek"
base00: "1d2021"
base01: "3c3836"
base02: "504945"
base03: "665c54"
base04: "bdae93"
base05: "d5c4a1"
base06: "ebdbb2"
base07: "fbf1c7"
base08: "fb4934"
base09: "fe8019"
base0A: "fabd2f"
base0B: "b8bb26"
base0C: "8ec07c"
base0D: "83a598"
base0E: "d3869b"
base0F: "d65d0e"
"#;

    #[test]
    fn parses_yaml_scheme() {
        let v = parse(YAML).expect("yaml scheme should parse");
        assert_eq!(v.len(), 16);
        assert_eq!(v[0], "1d2021");
        // base0A is the one that proves the key search is case-insensitive AND that the 'a'
        // in the key itself was not mistaken for the start of a colour.
        assert_eq!(v[10], "fabd2f");
        assert_eq!(v[15], "d65d0e");
    }

    #[test]
    fn parses_hash_prefixed_and_bare_list() {
        let hashed = YAML.replace("\"", "#");
        assert_eq!(parse(&hashed).unwrap()[0], "1d2021");

        let bare: String = (0..16)
            .map(|i| format!("#{:02}00ff\n", i))
            .collect::<Vec<_>>()
            .concat();
        let v = parse(&bare).expect("sixteen bare triples should parse");
        assert_eq!(v[0], "0000ff");
        assert_eq!(v[15], "1500ff");
    }

    #[test]
    fn rejects_partial_and_ambiguous_input() {
        // Fifteen keys: rejected rather than padded.
        let short = YAML.replace("base0F: \"d65d0e\"", "");
        assert!(parse(&short).is_none());
        assert!(parse("").is_none());
        assert!(parse("not a scheme at all").is_none());
        // Seventeen bare triples is ambiguous — we cannot know which sixteen were meant.
        let too_many: String = (0..17)
            .map(|i| format!("{:02}00ff\n", i))
            .collect::<Vec<_>>()
            .concat();
        assert!(parse(&too_many).is_none());
    }

    #[test]
    fn ignores_hex_runs_that_are_not_six_long() {
        // `deadbeef` is 8 hex digits: not a colour, must not be truncated into one.
        let s = format!("id: deadbeef\n{YAML}");
        assert_eq!(parse(&s).unwrap()[0], "1d2021");
    }
}
