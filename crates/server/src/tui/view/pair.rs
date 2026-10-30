//! The Pair card: a QR code built from block glyphs in the panel's colours ([`super::qr`]) —
//! compact by default, `z` for large — the same code to type, how long it has, then the grants
//! it carries and the link. A code that ran out or was revoked is not drawn: `n` makes another.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use super::qr::Qr;
use super::{card, row, toggle};
use crate::gate::{CodeState, typed_code};
use crate::tui::app::{App, Card, GRANTS, Target};
use crate::tui::theme;

/// Rows under the code: typed code, its state, gap, grants title, grants, gap, link (wrapped),
/// hint.
const BELOW: u16 = 10;
/// The narrowest the card gets: the row of grants.
const MIN_W: u16 = 44;
/// The large code's light: the terminal's own white, light on dark themes.
const LIGHT: Color = Color::White;

/// The QR, while its code still works.
fn qr(app: &App) -> Option<Qr> {
    if !matches!(app.pair_state, CodeState::Live(_)) {
        return None;
    }
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
    if let Some(a) = app.snap.asking.first() {
        return check(f, app, a, inner);
    }

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
            y += qh;
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
        (Ok(_), None) => {
            f.render_widget(
                Line::styled(" No live code", theme::bold()),
                Rect::new(inner.x, y, inner.width, 1),
            );
            y += 1;
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
    }

    // The same code to type, and how long it has.
    if let Ok(p) = &app.pair {
        let live = matches!(app.pair_state, CodeState::Live(_));
        if live && y < inner.bottom() {
            f.render_widget(
                Line::styled(
                    format!("type  {}", typed_code::display(&p.code.typed)),
                    theme::key(),
                )
                .centered(),
                Rect::new(inner.x, y, inner.width, 1),
            );
            y += 1;
        }
        if y < inner.bottom() {
            let (text, style) = if live {
                (app.pair_state.say(), theme::dim())
            } else {
                (
                    format!("{} · n", app.pair_state.say()),
                    Style::new().fg(theme::WARN),
                )
            };
            f.render_widget(
                Line::styled(text, style).centered(),
                Rect::new(inner.x, y, inner.width, 1),
            );
            y += 2;
        }
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
            4 => (name.to_string(), g.input),
            6 => (name.to_string(), g.join),
            7 => (name.to_string(), g.takeover),
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
            Line::styled("single use, 10 min · n new · c copy · z size", theme::dim()),
        ])
        .wrap(ratatui::widgets::Wrap { trim: true });
        let r = Rect::new(inner.x + 1, y, inner.width - 2, bottom.saturating_sub(y));
        f.render_widget(p, r);
    }
}

/// A device typed a code: its check words, to compare with its screen, and the answer keys.
fn check(f: &mut Frame, app: &App, a: &crate::gate::confirm::Ask, inner: Rect) {
    let words = a.words.split(' ').collect::<Vec<_>>().join("   ");
    let lines = [
        Line::styled(format!("{} typed a code", a.device), theme::bold()).centered(),
        Line::raw(""),
        Line::styled("Does it show these four words?", theme::dim()).centered(),
        Line::raw(""),
        Line::styled(words, theme::key()).centered(),
        Line::raw(""),
    ];
    let mut y = inner.y + inner.height.saturating_sub(lines.len() as u16 + 4) / 2;
    for l in lines {
        f.render_widget(l, Rect::new(inner.x, y, inner.width, 1));
        y += 1;
    }
    let btn = |k: &str, what: &str, t: Target| {
        vec![
            (Span::styled(format!(" {k} "), theme::cursor()), Some(t)),
            (Span::styled(format!(" {what}   "), theme::bold()), Some(t)),
        ]
    };
    let mut spans = btn("y", "they match", Target::Check(true));
    spans.extend(btn("n", "they don't", Target::Check(false)));
    let w: u16 = spans.iter().map(|(s, _)| s.width() as u16).sum();
    row(
        f,
        app,
        inner.x + inner.width.saturating_sub(w) / 2,
        y,
        inner.right(),
        spans,
    );
    y += 2;
    if y < inner.bottom() {
        f.render_widget(
            Line::styled(
                format!("{} s left · no answer counts as no", a.left()),
                theme::dim(),
            )
            .centered(),
            Rect::new(inner.x, y, inner.width, 1),
        );
    }
}
