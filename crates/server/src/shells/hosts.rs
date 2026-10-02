//! The ssh hosts on offer: every concrete `Host` alias in the host's `~/.ssh/config`.
//!
//! The list is also the allow-list. An ssh tab can only name an alias that appears here, so a
//! client cannot turn "open an ssh tab" into "run ssh with arguments of my choosing".
//!
//! ponytail: `Include` lines are not followed. Follow them if someone's hosts live in one.

pub fn list() -> Vec<String> {
    let Some(home) = std::env::var_os("HOME") else {
        return Vec::new();
    };
    let path = std::path::Path::new(&home).join(".ssh/config");
    parse(&std::fs::read_to_string(path).unwrap_or_default())
}

pub fn parse(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        let mut words = line.split_whitespace();
        if !words.next().is_some_and(|k| k.eq_ignore_ascii_case("host")) {
            continue;
        }
        for alias in words {
            if valid(alias) && !out.iter().any(|a| a == alias) {
                out.push(alias.to_string());
            }
        }
    }
    out
}

/// A plain alias: no patterns (`*`, `?`, `!`), nothing ssh would read as an option.
pub fn valid(alias: &str) -> bool {
    !alias.is_empty()
        && !alias.starts_with('-')
        && alias
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "._-@".contains(c))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn concrete_aliases_only() {
        let cfg = "Host nas pi\n  HostName 10.0.0.7\nhost *.lan\nHost !bad -oProxy x\nMatch all\nHost nas\n";
        assert_eq!(parse(cfg), ["nas", "pi", "x"]);
        assert!(!valid("-oProxyCommand=sh"));
        assert!(!valid("a;b"));
    }
}
