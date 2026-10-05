//! The Screens card: one tile per daemon. A running session is drawn as the screen it is — its
//! real proportions, a phone tall and narrow, a laptop wide — beside who watches it and what is
//! open. An idle daemon is a dashed outline waiting for the next device.

use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, BorderType, Paragraph},
};
use wado_protocol::SessionSummary;

use super::card;
use crate::tui::app::App;
use crate::tui::theme;

const TILE: u16 = 26;

pub fn draw(f: &mut Frame, app: &App, area: Rect) {
    let block = card("Screens", false);
    let inner = block.inner(area);
    f.render_widget(block, area);
    let daemons = &app.snap.status.daemons;
    if daemons.is_empty() {
        f.render_widget(
            Line::styled(" no daemons — scripts/rig.sh starts them", theme::dim()),
            inner,
        );
        return;
    }
    // Busy daemons first, so a crowded rig still shows who is on.
    let mut order: Vec<&String> = daemons.iter().collect();
    order.sort_by_key(|d| app.snap.session_on(d).is_none());
    let fit = (inner.width / TILE).max(1) as usize;
    let tw = inner.width / order.len().min(fit) as u16;
    for (i, d) in order.iter().take(fit).enumerate() {
        let r = Rect::new(inner.x + i as u16 * tw, inner.y, tw, inner.height);
        tile(f, d, app.snap.session_on(d), r);
    }
    if order.len() > fit {
        let more = format!("+{} ", order.len() - fit);
        let w = more.len() as u16;
        f.render_widget(
            Span::styled(more, theme::dim()),
            Rect::new(inner.right() - w, inner.bottom() - 1, w, 1),
        );
    }
}

fn tile(f: &mut Frame, inst: &str, s: Option<&SessionSummary>, r: Rect) {
    let (sw, sh) = s.map_or((1080, 1920), |s| (s.width, s.height));
    // The words beside it need ~18 columns; the drawing gets the rest, never more than 12 rows.
    let (cw, ch) = fit(sw, sh, r.width.saturating_sub(20).max(6), r.height.min(12));
    let top = r.y + (r.height - ch) / 2;
    let screen = Rect::new(r.x + 1, top, cw, ch);
    let text_x = screen.right() + 2;
    let text = Rect::new(
        text_x,
        top,
        r.right().saturating_sub(text_x + 1),
        r.bottom() - top,
    );
    match s {
        Some(s) => {
            f.render_widget(
                Paragraph::new(Line::styled(theme::PLAY, Style::new().fg(theme::GOOD)))
                    .centered()
                    .block(
                        Block::bordered()
                            .border_type(BorderType::Rounded)
                            .border_style(Style::new().fg(theme::GOOD)),
                    ),
                screen,
            );
            let mut l = vec![
                Line::from(vec![
                    Span::styled(format!("#{inst} "), theme::dim()),
                    Span::styled(
                        s.viewer.clone().unwrap_or_else(|| "detached".into()),
                        theme::bold(),
                    ),
                ]),
                Line::styled(format!("{}×{}", s.width, s.height), theme::accent()),
                Line::styled(
                    format!("{} fps · up {}", s.fps, uptime(s.age_s)),
                    theme::dim(),
                ),
            ];
            if s.apps.is_empty() {
                l.push(Line::styled("no windows", theme::dim()));
            }
            l.extend(s.apps.iter().map(|a| Line::raw(format!("▪ {a}"))));
            f.render_widget(Paragraph::new(l), text);
        }
        None => {
            f.render_widget(
                Block::bordered()
                    .border_type(BorderType::LightDoubleDashed)
                    .border_style(theme::dim()),
                screen,
            );
            f.render_widget(
                Paragraph::new(vec![
                    Line::from(vec![
                        Span::styled(format!("#{inst} "), theme::dim()),
                        Span::styled("idle", theme::bold()),
                    ]),
                    Line::styled("the next device", theme::dim()),
                    Line::styled("lands here", theme::dim()),
                ]),
                text,
            );
        }
    }
}

/// The largest `cols × rows` box with the proportions of a `w × h` screen inside
/// `max_c × max_r` cells, a cell being twice as tall as it is wide.
fn fit(w: u32, h: u32, max_c: u16, max_r: u16) -> (u16, u16) {
    let (w, h) = (w.max(1) as f64, h.max(1) as f64);
    let mut rows = max_r as f64;
    let mut cols = rows * 2.0 * w / h;
    if cols > max_c as f64 {
        cols = max_c as f64;
        rows = cols * h / (2.0 * w);
    }
    (
        (cols.round() as u16).clamp(3, max_c.max(3)),
        (rows.round() as u16).clamp(3, max_r.max(3)),
    )
}

pub fn uptime(s: u64) -> String {
    match s {
        0..60 => format!("{s}s"),
        60..3600 => format!("{}m", s / 60),
        _ => format!("{}h{:02}m", s / 3600, s % 3600 / 60),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_screen_keeps_its_shape() {
        // A phone in portrait is taller than wide on screen, a laptop wider than tall.
        let (c, r) = super::fit(1080, 2400, 14, 7);
        assert!(c < r * 2, "{c}×{r}");
        let (c, r) = super::fit(1920, 1080, 14, 7);
        assert!(c > r * 2, "{c}×{r}");
        assert!(c <= 14 && r <= 7);
    }
}
