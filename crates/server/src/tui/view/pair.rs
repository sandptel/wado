//! The Pair card: a QR code that always works, built from block glyphs in the panel's colours
//! ([`super::qr`]) — compact by default, `z` for large — then the grants it carries and the link.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use super::qr::Qr;
use super::{card, row, toggle};
use crate::tui::app::{App, Card, GRANTS, Target};
use crate::tui::theme;

/// Rows under the code: gap, grants title, grants, gap, link (wrapped), hint.
const BELOW: u16 = 8;
/// The narrowest the card gets: the row of grants.
const MIN_W: u16 = 44;
/// The large code's light: the terminal's own white, light on dark themes.
const LIGHT: Color = Color::White;

fn qr(app: &App) -> Option<Qr> {
    Qr::new(&app.pair.as_ref().ok()?.link)
}

/// The card's width: the code's, at the size that fits in `height`.
pub fn width(app: &App, height: u16) -> u16 {
    let fits = |large| {
        qr(app)
            .map(|q| q.cells(large))
            .filter(|(_, h)| h + BELOW + 2 <= height)
    };
    let (w, _) = fits(app.big_qr).or_else(|| fits(false)).unwrap_or((0, 0));
    (w + 4).max(MIN_W)
}

pub fn draw(f: &mut Frame, app: &App, area: Rect) {
    let focused = app.focus == Card::Pair;
    let block = card("Pair a device", focused);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let code = qr(app);
    // Large only when asked for and there is room; compact otherwise.
    let large = app.big_qr
        && code
            .as_ref()
            .is_some_and(|q| q.cells(true).1 + BELOW <= inner.height);
    let (qw, qh) = code.as_ref().map_or((0, 0), |q| q.cells(large));
    // Centred top to bottom.
    let mut y = inner.y + inner.height.saturating_sub(qh + BELOW) / 2;
    match (&app.pair, &code) {
        (Ok(_), Some(q)) if qh + BELOW <= inner.height => {
            let r = Rect::new(
                inner.x + inner.width.saturating_sub(qw) / 2,
                y,
                qw.min(inner.width),
                qh,
            );
            let ink = if large {
                Style::new().fg(LIGHT)
            } else {
                theme::accent()
            };
            f.render_widget(Paragraph::new(q.lines(large, ink)), r);
            app.hits.borrow_mut().push((r, Target::Zoom));
            y += qh + 1;
        }
        (Ok(_), Some(_)) => {
            f.render_widget(
                Line::styled(
                    format!(" ↕ {} more rows for the QR code", qh + BELOW - inner.height),
                    Style::new().fg(theme::WARN),
                ),
                Rect::new(inner.x, y, inner.width, 1),
            );
            y += 2;
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
            3 => (name.to_string(), g.host),
            _ => (name.to_string(), g.clipboard),
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
                "single use, renewed once scanned · c copy · z size",
                theme::dim(),
            ),
        ])
        .wrap(ratatui::widgets::Wrap { trim: true });
        let r = Rect::new(inner.x + 1, y, inner.width - 2, bottom.saturating_sub(y));
        f.render_widget(p, r);
    }
}
