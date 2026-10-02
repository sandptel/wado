//! The desktop's colour behind every window.
//!
//! The viewer sends its scheme's colour (`SessionConfig::background`) so an empty session looks
//! like part of the shell around it, not a black hole that reads as "the stream is broken".

/// A slate blue-grey: unmistakably a desktop, not a dead picture.
pub const DEFAULT_BACKGROUND: [f32; 4] = [
    0x2b as f32 / 255.0,
    0x33 as f32 / 255.0,
    0x40 as f32 / 255.0,
    1.0,
];

/// `rrggbb` (with or without `#`) → RGBA; anything else is the default.
pub fn background(hex: Option<&str>) -> [f32; 4] {
    let Some(h) = hex
        .map(|h| h.trim_start_matches('#'))
        .filter(|h| h.len() == 6)
    else {
        return DEFAULT_BACKGROUND;
    };
    let ch = |i: usize| {
        u8::from_str_radix(&h[i..i + 2], 16)
            .ok()
            .map(|v| v as f32 / 255.0)
    };
    match (ch(0), ch(2), ch(4)) {
        (Some(r), Some(g), Some(b)) => [r, g, b, 1.0],
        _ => DEFAULT_BACKGROUND,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_falls_back() {
        assert_eq!(background(Some("#ff0000")), [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(background(Some("zz0000")), DEFAULT_BACKGROUND);
        assert_eq!(background(None), DEFAULT_BACKGROUND);
        assert!(
            DEFAULT_BACKGROUND[..3].iter().any(|c| *c > 0.15),
            "not black"
        );
    }
}
