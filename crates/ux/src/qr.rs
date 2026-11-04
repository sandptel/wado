//! A QR code as an SVG `path` over a square of `size` modules — the web draws it inline, any
//! client can; light quiet zone included in `size`.

use qrcode::{Color, QrCode};

/// `(size, path d)`; `None` when the text does not fit a QR code.
pub fn path(text: &str) -> Option<(usize, String)> {
    let code = QrCode::new(text.as_bytes()).ok()?;
    let w = code.width();
    let quiet = 2;
    let mut d = String::new();
    for (i, c) in code.to_colors().iter().enumerate() {
        if *c == Color::Dark {
            let (x, y) = (i % w + quiet, i / w + quiet);
            d.push_str(&format!("M{x} {y}h1v1h-1z"));
        }
    }
    Some((w + 2 * quiet, d))
}

#[cfg(test)]
mod tests {
    #[test]
    fn draws() {
        let (n, d) = super::path("https://example.com/?pair=abc").unwrap();
        assert!(n > 20 && d.starts_with('M'));
    }
}
