//! The Load card: each daemon's CPU over the last two minutes as a braille area graph — green at
//! the floor, red at the peak — with its memory beside it.

use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
};

use super::card;
use crate::tui::app::App;
use crate::tui::theme;

pub fn draw(f: &mut Frame, app: &App, area: Rect) {
    let block = card("Load", false);
    let inner = block.inner(area);
    f.render_widget(block, area);
    let ds = &app.load.daemons;
    if ds.is_empty() {
        f.render_widget(Line::styled(" no daemons", theme::dim()), inner);
        return;
    }
    let each = (inner.height / ds.len() as u16).max(1);
    for (i, d) in ds.iter().enumerate() {
        let y = inner.y + i as u16 * each;
        if y >= inner.bottom() {
            break;
        }
        let now = d.cpu.back().copied().unwrap_or(0.0);
        let label = Line::from(vec![
            Span::styled(format!(" #{} ", d.instance), theme::dim()),
            Span::styled(format!("cpu {now:>4.1}%"), theme::bold()),
            Span::styled(format!("  mem {}", mib(d.rss)), theme::dim()),
        ]);
        f.render_widget(label, Rect::new(inner.x, y, inner.width, 1));
        let gh = each.saturating_sub(1).min(inner.bottom() - y - 1);
        if gh == 0 {
            continue;
        }
        let w = inner.width.saturating_sub(2) as usize;
        let data: Vec<f64> = d.cpu.iter().copied().collect();
        let tail = &data[data.len().saturating_sub(w * 2)..];
        // Scale to the window's peak, never finer than 10%, so idle reads as idle.
        let max = tail.iter().copied().fold(10.0, f64::max);
        for (r, s) in braille(tail, max, w, gh as usize).into_iter().enumerate() {
            let heat = r as f64 / gh as f64;
            let c = if gh > 1 && heat < 0.34 {
                theme::BAD
            } else if gh > 1 && heat < 0.67 {
                theme::WARN
            } else {
                theme::GOOD
            };
            f.render_widget(
                Span::styled(s, Style::new().fg(c)),
                Rect::new(inner.x + 1, y + 1 + r as u16, w as u16, 1),
            );
        }
    }
}

fn mib(b: u64) -> String {
    format!("{}M", b / (1 << 20))
}

/// `data` as `rows` lines of `width` braille cells, filled from the bottom, two samples a cell,
/// right-aligned so the newest sample is at the right edge.
fn braille(data: &[f64], max: f64, width: usize, rows: usize) -> Vec<String> {
    const LEFT: [u32; 4] = [0x40, 0x04, 0x02, 0x01];
    const RIGHT: [u32; 4] = [0x80, 0x20, 0x10, 0x08];
    let dots = rows * 4;
    let level = |v: f64| ((v / max).clamp(0.0, 1.0) * dots as f64).round() as usize;
    let pad = (width * 2).saturating_sub(data.len());
    let at = |i: usize| {
        i.checked_sub(pad)
            .and_then(|i| data.get(i))
            .map_or(0, |v| level(*v))
    };
    (0..rows)
        .map(|r| {
            let base = (rows - 1 - r) * 4;
            (0..width)
                .map(|c| {
                    let (l, rt) = (at(c * 2), at(c * 2 + 1));
                    let bits = (0..4).fold(0, |b, i| {
                        b | if l > base + i { LEFT[i] } else { 0 }
                            | if rt > base + i { RIGHT[i] } else { 0 }
                    });
                    char::from_u32(0x2800 + bits).unwrap_or(' ')
                })
                .collect()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn braille_fills_from_the_bottom() {
        let g = super::braille(&[10.0, 10.0, 0.0, 0.0], 10.0, 2, 2);
        assert_eq!(g, vec!["⣿⠀", "⣿⠀"]);
        let g = super::braille(&[5.0, 5.0], 10.0, 1, 2);
        assert_eq!(g, vec!["⠀", "⣿"]);
    }
}
