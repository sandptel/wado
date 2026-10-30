//! How a clipboard entry is said: where it was copied and when, for the eye and for a screen
//! reader ("Text: meeting at 4. Copied on Laptop, 2 min ago").

use wado_protocol::{ClipEntry, ClipKind};

/// Where it was copied, in words: the computer's desktop is the computer, a wado session is
/// said as one, and a device by its name. Empty when the history does not say.
pub fn origin(from: &str, computer: &str) -> String {
    match from {
        "" => String::new(),
        "desktop" => computer.to_string(),
        "session" => format!("{computer}'s wado session"),
        device => device.to_string(),
    }
}

/// "from Pixel · 2 min ago", "on Laptop", or "" — the visible meta line.
pub fn meta(e: &ClipEntry, computer: &str) -> String {
    let ago = crate::profile::ago(e.at_ms / 1000);
    let from = origin(&e.from, computer);
    match (from.is_empty(), ago.is_empty()) {
        (false, false) => format!("{from} · {ago}"),
        (false, true) => from,
        (true, false) => ago,
        (true, true) => String::new(),
    }
}

pub fn kind(k: ClipKind) -> &'static str {
    match k {
        ClipKind::Text => "Text",
        ClipKind::Link => "Link",
        ClipKind::Image => "Picture",
    }
}

/// The whole entry, as a screen reader says it.
pub fn label(e: &ClipEntry, computer: &str) -> String {
    let what: String = e.preview.chars().take(120).collect();
    let from = origin(&e.from, computer);
    let ago = crate::profile::ago(e.at_ms / 1000);
    let mut out = format!("{}: {what}.", kind(e.kind));
    if e.pinned {
        out.push_str(" Pinned.");
    }
    match (from.is_empty(), ago.is_empty()) {
        (false, false) => out.push_str(&format!(" Copied on {from}, {ago}.")),
        (false, true) => out.push_str(&format!(" Copied on {from}.")),
        (true, false) => out.push_str(&format!(" Copied {ago}.")),
        (true, true) => out.push_str(&format!(" On {computer}'s clipboard.")),
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(from: &str) -> ClipEntry {
        ClipEntry {
            id: "1".into(),
            kind: ClipKind::Link,
            preview: "https://wado.dev".into(),
            pinned: true,
            at_ms: 0,
            from: from.into(),
        }
    }

    #[test]
    fn entries_are_said_with_where_they_came_from() {
        assert_eq!(
            label(&entry("desktop"), "Laptop"),
            "Link: https://wado.dev. Pinned. Copied on Laptop."
        );
        assert_eq!(origin("session", "Laptop"), "Laptop's wado session");
        assert_eq!(origin("Pixel 8", "Laptop"), "Pixel 8");
        assert_eq!(
            label(&entry(""), "Laptop"),
            "Link: https://wado.dev. Pinned. On Laptop's clipboard."
        );
        assert_eq!(meta(&entry(""), "Laptop"), "");
    }
}
