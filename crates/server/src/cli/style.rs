//! Colour for the CLI's own output — off when stdout is not a terminal or `NO_COLOR` is set.

use std::{io::IsTerminal, sync::OnceLock};

fn on() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none())
}

fn paint(code: &str, s: &str) -> String {
    if on() {
        format!("\x1b[{code}m{s}\x1b[0m")
    } else {
        s.to_string()
    }
}

pub fn bold(s: &str) -> String {
    paint("1", s)
}
pub fn dim(s: &str) -> String {
    paint("2", s)
}
pub fn accent(s: &str) -> String {
    paint("1;36", s)
}
pub fn good(s: &str) -> String {
    paint("32", s)
}
pub fn bad(s: &str) -> String {
    paint("31", s)
}
pub fn warn(s: &str) -> String {
    paint("33", s)
}

/// `●` green when up, `○` dim when down.
pub fn dot(up: bool) -> String {
    if up { good("●") } else { dim("○") }
}
