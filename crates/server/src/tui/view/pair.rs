//! The Pair card: a QR code that always works, drawn module by module in half-blocks, black on
//! white whatever the terminal's theme — then the row of grants it carries and the link.

use qrcode::{Color as Module, EcLevel, QrCode};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use super::{card, row, toggle};
use crate::tui::app::{App, Card, GRANTS, Target};
use crate::tui::theme;

/// Light modules around the code; scanners want some, a screen needs few.
const QUIET: usize = 2;
/// Rows under the code: gap, grants title, grants, gap, link, hint.
const BELOW: u16 = 6;
const BLACK: Color = Color::Indexed(16);
const WHITE: Color = Color::Indexed(231);

/// The card's width: the code's when it fits in `height`, else enough for the grants.
pub fn width(app: &App, height: u16) -> u16 {
    let side = modules(app).map_or(0, |(w, _)| (w + 2 * QUIET) as u16);
    if side.div_ceil(2) + BELOW + 2 > height {
        return 48;
    }
    (side + 4).max(48)
}

fn modules(app: &App) -> Option<(usize, Vec<Module>)> {
    let link = &app.pair.as_ref().ok()?.link;
    // Low correction: a screen does not get scratched, and it keeps the code small.
    let code = QrCode::with_error_correction_level(link.as_bytes(), EcLevel::L).ok()?;
    Some((code.width(), code.to_colors()))
}

/// The code as lines, two modules per cell: `▀` coloured top by foreground, bottom by background.
fn qr_lines(w: usize, m: &[Module]) -> Vec<Line<'static>> {
    let side = w + 2 * QUIET;
    let dark = |x: usize, y: usize| {
        x >= QUIET
            && y >= QUIET
            && x - QUIET < w
            && y - QUIET < w
            && m[(y - QUIET) * w + (x - QUIET)] == Module::Dark
    };
    let c = |d: bool| if d { BLACK } else { WHITE };
    (0..side)
        .step_by(2)
        .map(|y| {
            Line::from(
                (0..side)
                    .map(|x| {
                        Span::styled("▀", Style::new().fg(c(dark(x, y))).bg(c(dark(x, y + 1))))
                    })
                    .collect::<Vec<_>>(),
            )
        })
        .collect()
}

pub fn draw(f: &mut Frame, app: &App, area: Rect) {
    let focused = app.focus == Card::Pair;
    let block = card("Pair a device", focused);
    let inner = block.inner(area);
    f.render_widget(block, area);

    // Centred top to bottom: the link wraps to about three lines.
    let side = modules(app).map_or(0, |(w, _)| (w + 2 * QUIET) as u16);
    let content = side.div_ceil(2) + BELOW + 2;
    let mut y = inner.y + inner.height.saturating_sub(content) / 2;
    match (&app.pair, modules(app)) {
        (Ok(_), Some((w, m))) => {
            let lines = qr_lines(w, &m);
            let (h, w) = (lines.len() as u16, (w + 2 * QUIET) as u16);
            if inner.height >= h + BELOW {
                let x = inner.x + inner.width.saturating_sub(w) / 2;
                f.render_widget(
                    Paragraph::new(lines),
                    Rect::new(x, y, inner.width.min(w), h),
                );
                y += h + 1;
            } else {
                f.render_widget(
                    Line::styled(
                        format!(
                            " ↕ {} more rows to show the QR code",
                            h + BELOW - inner.height
                        ),
                        Style::new().fg(theme::WARN),
                    ),
                    Rect::new(inner.x, y, inner.width, 1),
                );
                y += 2;
            }
        }
        (Err(e), _) => {
            let p = Paragraph::new(vec![
                Line::styled(" No code yet", theme::bold()),
                Line::styled(format!(" {e}"), theme::dim()),
            ])
            .wrap(ratatui::widgets::Wrap { trim: true });
            f.render_widget(p, Rect::new(inner.x, y, inner.width, 5));
            y += 6;
        }
        _ => {}
    }

    // The grants it carries, one toggle each.
    let bottom = inner.bottom();
    if y >= bottom {
        return;
    }
    f.render_widget(
        Line::styled(" whoever scans it may use", theme::dim()),
        Rect::new(inner.x, y, inner.width, 1),
    );
    y += 1;
    if y >= bottom {
        return;
    }
    let g = &app.pair_grants;
    let cur = app.at(Card::Pair);
    let mut spans = vec![(Span::raw(" "), None)];
    for (i, name) in GRANTS.iter().enumerate() {
        let (label, on) = match i {
            0 => (
                match g.files {
                    "ro" => "files·read".to_string(),
                    "rw" => "files·write".to_string(),
                    _ => "files".to_string(),
                },
                g.files != "none",
            ),
            1 => (name.to_string(), g.shells),
            2 => (name.to_string(), g.settings),
            _ => (name.to_string(), g.host),
        };
        let mark = if on { theme::ON } else { theme::OFF };
        spans.push((
            toggle(
                format!("{mark} {label}"),
                on,
                cur == Some((0, i)),
                label.chars().count() + 4,
            ),
            Some(Target::Cell(Card::Pair, 0, i)),
        ));
    }
    row(f, app, inner.x, y, inner.right(), spans);
    y += 2;

    if let Ok(p) = &app.pair {
        let short = p
            .link
            .split("&pair=")
            .next()
            .unwrap_or("")
            .trim_start_matches("https://");
        let p = Paragraph::new(vec![
            Line::styled(short.to_string(), theme::accent()),
            Line::styled(
                "single use, renewed once scanned · c copies it",
                theme::dim(),
            ),
        ])
        .wrap(ratatui::widgets::Wrap { trim: true });
        let r = Rect::new(inner.x + 1, y, inner.width - 2, bottom.saturating_sub(y));
        f.render_widget(p, r);
    }
}

#[cfg(test)]
mod tests {
    use qrcode::{Color as Module, QrCode};

    #[test]
    fn half_blocks_carry_every_module() {
        let code = QrCode::new(b"https://example.com/?id=872990894").unwrap();
        let (w, m) = (code.width(), code.to_colors());
        let lines = super::qr_lines(w, &m);
        let side = w + 2 * super::QUIET;
        assert_eq!(lines.len(), side.div_ceil(2));
        for (y, x) in (0..w).flat_map(|y| (0..w).map(move |x| (y, x))) {
            let (cy, cx) = (y + super::QUIET, x + super::QUIET);
            let st = lines[cy / 2].spans[cx].style;
            let c = if cy % 2 == 0 { st.fg } else { st.bg };
            let dark = m[y * w + x] == Module::Dark;
            assert_eq!(
                c,
                Some(if dark { super::BLACK } else { super::WHITE }),
                "{x},{y}"
            );
        }
    }
}
