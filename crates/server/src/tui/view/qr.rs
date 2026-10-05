//! A QR code built from text: block glyphs in the terminal's own colours, no picture pasted in.
//!
//! Light modules are drawn in ink on the terminal's background, which reads as dark — so on a
//! dark theme it is an ordinary dark-on-light code to a camera. Two sizes:
//!
//! - **compact** — sextants, 2×3 modules a cell: a third the area, modules slightly tall, which
//!   a camera corrects like any tilt.
//! - **large** — half-blocks, 1×2 a cell: square modules, for a camera that struggles.

use qrcode::{Color as Module, EcLevel, QrCode};
use ratatui::{style::Style, text::Line};

/// Light modules around the code. Scanners want a margin; on a screen a narrow one does.
const QUIET: usize = 2;

pub struct Qr {
    width: usize,
    modules: Vec<Module>,
}

impl Qr {
    pub fn new(text: &str) -> Option<Self> {
        // Low correction: a screen does not get scratched, and it keeps the code small.
        let code = QrCode::with_error_correction_level(text.as_bytes(), EcLevel::L).ok()?;
        Some(Self {
            width: code.width(),
            modules: code.to_colors(),
        })
    }

    fn side(&self) -> usize {
        self.width + 2 * QUIET
    }

    /// Whether `(x, y)` — margin included — is drawn in ink. Past the margin is nothing.
    fn ink(&self, x: usize, y: usize) -> bool {
        let (s, q, w) = (self.side(), QUIET, self.width);
        if x >= s || y >= s {
            return false;
        }
        if x < q || y < q || x - q >= w || y - q >= w {
            return true;
        }
        self.modules[(y - q) * w + (x - q)] == Module::Light
    }

    /// `(columns, rows)` it takes.
    pub fn cells(&self, large: bool) -> (u16, u16) {
        let s = self.side();
        if large {
            (s as u16, s.div_ceil(2) as u16)
        } else {
            (s.div_ceil(2) as u16, s.div_ceil(3) as u16)
        }
    }

    pub fn lines(&self, large: bool, ink: Style) -> Vec<Line<'static>> {
        let (cols, rows) = self.cells(large);
        (0..rows as usize)
            .map(|r| {
                let s: String = (0..cols as usize)
                    .map(|c| {
                        if large {
                            half(self.ink(c, r * 2), self.ink(c, r * 2 + 1))
                        } else {
                            let (x, y) = (c * 2, r * 3);
                            let bits = (0..6)
                                .fold(0, |b, i| b | (self.ink(x + i % 2, y + i / 2) as u32) << i);
                            sextant(bits)
                        }
                    })
                    .collect();
                Line::styled(s, ink)
            })
            .collect()
    }
}

fn half(top: bool, bottom: bool) -> char {
    match (top, bottom) {
        (false, false) => ' ',
        (true, false) => '▀',
        (false, true) => '▄',
        (true, true) => '█',
    }
}

/// The sextant with `bits` lit: bit 0 top-left, 1 top-right, 2 middle-left … 5 bottom-right.
/// Unicode leaves out the four that already exist: empty, the two half blocks, full.
fn sextant(bits: u32) -> char {
    match bits {
        0 => ' ',
        21 => '▌',
        42 => '▐',
        63 => '█',
        n => char::from_u32(0x1FB00 + n - 1 - (n > 21) as u32 - (n > 42) as u32).unwrap_or('?'),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Read the drawing back, module by module, in both sizes.
    #[test]
    fn every_module_survives_the_drawing() {
        let q = Qr::new("https://example.com/?id=872990894&pair=abc").unwrap();
        let unsextant = |ch: char| -> u32 {
            match ch {
                ' ' => 0,
                '▌' => 21,
                '▐' => 42,
                '█' => 63,
                c => {
                    let i = c as u32 - 0x1FB00 + 1;
                    i + (i >= 21) as u32 + (i + (i >= 21) as u32 >= 42) as u32
                }
            }
        };
        for large in [false, true] {
            let rows: Vec<Vec<char>> = q
                .lines(large, Style::new())
                .iter()
                .map(|l| l.to_string().chars().collect())
                .collect();
            for y in 0..q.side() {
                for x in 0..q.side() {
                    let got = if large {
                        let ch = rows[y / 2][x];
                        if y % 2 == 0 {
                            "▀█".contains(ch)
                        } else {
                            "▄█".contains(ch)
                        }
                    } else {
                        let bits = unsextant(rows[y / 3][x / 2]);
                        bits >> ((y % 3) * 2 + x % 2) & 1 == 1
                    };
                    assert_eq!(got, q.ink(x, y), "large={large} {x},{y}");
                }
            }
        }
    }
}
