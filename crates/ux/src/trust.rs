//! A trusted device in words: how long it has, what it may do.

/// "until removed", "ends in 12 min", "ends in 5 h", "ends in 3 d".
pub fn left(until: Option<u64>, now: u64) -> String {
    let Some(u) = until else {
        return "until removed".into();
    };
    match u.saturating_sub(now) {
        0 => "ending".into(),
        s if s < 3600 => format!("ends in {} min", s.div_ceil(60)),
        s if s < 86_400 => format!("ends in {} h", (s + 1800) / 3600),
        s => format!("ends in {} d", (s + 43_200) / 86_400),
    }
}

/// `clipboard,files-ro,input` → "clipboard, files (read), trackpad".
pub fn grants(tokens: &str) -> String {
    let words: Vec<&str> = tokens
        .split(',')
        .filter(|t| !t.is_empty() && *t != "-")
        .map(|t| match t {
            "files-ro" => "files (read)",
            "files-rw" => "files",
            "host" => "sound & sleep",
            "input" => "trackpad",
            "takeover" => "take over",
            "devices" => "manages devices",
            o => o,
        })
        .collect();
    if words.is_empty() {
        "the desktop only".into()
    } else {
        words.join(", ")
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn words() {
        assert_eq!(super::left(None, 5), "until removed");
        assert_eq!(super::left(Some(5 + 600), 5), "ends in 10 min");
        assert_eq!(super::grants("clipboard,input"), "clipboard, trackpad");
        assert_eq!(super::grants("-"), "the desktop only");
    }
}
