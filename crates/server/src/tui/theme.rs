//! Colours and glyphs, in one place so the panel reads as one thing. Terminal palette colours,
//! not RGB: they follow the user's terminal theme, light or dark.

use ratatui::style::{Color, Modifier, Style};

pub const ACCENT: Color = Color::Cyan;
pub const GOOD: Color = Color::Green;
pub const WARN: Color = Color::Yellow;
pub const BAD: Color = Color::Red;
pub const DIM: Color = Color::DarkGray;

pub const ONLINE: &str = "●";
pub const OFFLINE: &str = "○";
pub const OWNER: &str = "★";
pub const PINNED: &str = "◆";
pub const UNPINNED: &str = "◇";
pub const ON: &str = "✓";
pub const OFF: &str = "·";
pub const PLAY: &str = "▶";
pub const WAIT: &str = "⧗";
pub const ALERT: &str = "⚠";

pub fn dim() -> Style {
    Style::new().fg(DIM)
}
pub fn bold() -> Style {
    Style::new().add_modifier(Modifier::BOLD)
}
pub fn accent() -> Style {
    Style::new().fg(ACCENT)
}
pub fn key() -> Style {
    Style::new().fg(ACCENT).add_modifier(Modifier::BOLD)
}
/// The selected row of the focused list.
pub fn selected() -> Style {
    Style::new()
        .bg(Color::Indexed(236))
        .add_modifier(Modifier::BOLD)
}
/// A panel's border: accent when focused.
pub fn border(focused: bool) -> Style {
    if focused { accent() } else { dim() }
}
/// `●` green or `○` grey.
pub fn dot(up: bool) -> (&'static str, Style) {
    if up {
        (ONLINE, Style::new().fg(GOOD))
    } else {
        (OFFLINE, dim())
    }
}
