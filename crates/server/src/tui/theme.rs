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

/// The cursor in the focused card.
pub fn cursor() -> Style {
    Style::new()
        .fg(Color::Black)
        .bg(ACCENT)
        .add_modifier(Modifier::BOLD)
}

/// A grant or switch: green when on, grey when off.
pub fn state(on: bool) -> Style {
    if on { Style::new().fg(GOOD) } else { dim() }
}

/// `text` in a three-row box-drawing face — digits, `-`, space and the letters of "wado".
pub fn big(text: &str) -> [String; 3] {
    let mut rows = [String::new(), String::new(), String::new()];
    for (i, c) in text.chars().enumerate() {
        let g: [&str; 3] = match c {
            '0' | 'o' => ["┏━┓", "┃ ┃", "┗━┛"],
            '1' => ["╺┓ ", " ┃ ", "╺┻╸"],
            '2' => ["┏━┓", "┏━┛", "┗━╸"],
            '3' => ["┏━┓", "╺━┫", "┗━┛"],
            '4' => ["╻ ╻", "┗━┫", "  ╹"],
            '5' => ["┏━╸", "┗━┓", "┗━┛"],
            '6' => ["┏━┓", "┣━┓", "┗━┛"],
            '7' => ["┏━┓", "  ┃", "  ╹"],
            '8' => ["┏━┓", "┣━┫", "┗━┛"],
            '9' => ["┏━┓", "┗━┫", "┗━┛"],
            'w' => ["╻ ╻", "┃╻┃", "┗┻┛"],
            'a' => ["┏━┓", "┣━┫", "╹ ╹"],
            'd' => ["╺┳┓", " ┃┃", "╺┻┛"],
            '-' => ["   ", "╺━╸", "   "],
            _ => [" ", " ", " "],
        };
        for (r, part) in rows.iter_mut().zip(g) {
            if i > 0 {
                r.push(' ');
            }
            r.push_str(part);
        }
    }
    rows
}
