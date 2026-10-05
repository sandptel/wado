//! The Activity card: who came in and what they did, newest at the bottom, one glyph per kind.

use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
};

use super::card;
use crate::tui::app::App;
use crate::tui::theme;

pub fn draw(f: &mut Frame, app: &App, area: Rect) {
    let block = card("Activity", false);
    let inner = block.inner(area);
    f.render_widget(block, area);
    let lines: Vec<Line> = app
        .snap
        .activity
        .iter()
        .rev()
        .take(inner.height as usize)
        .rev()
        .map(|l| line(l))
        .collect();
    if lines.is_empty() {
        f.render_widget(
            Line::styled(
                " quiet — joins, grants, files and shells show here",
                theme::dim(),
            ),
            inner,
        );
        return;
    }
    f.render_widget(Paragraph::new(lines), inner);
}

/// `HH:MM:SS #N kind: message` → time, daemon, a glyph for the kind, the message coloured by
/// how it went.
fn line(l: &str) -> Line<'static> {
    let mut parts = l.splitn(3, ' ');
    let (time, daemon, rest) = (
        parts.next().unwrap_or(""),
        parts.next().unwrap_or(""),
        parts.next().unwrap_or(""),
    );
    let (glyph, msg) = [
        ("gate:", "⇥"),
        ("files:", "▤"),
        ("shells:", "❯"),
        ("config changed", "⚙"),
        ("host action", "⌂"),
    ]
    .iter()
    .find(|(k, _)| rest.starts_with(k))
    .map_or(("·", rest), |(k, g)| {
        (
            *g,
            rest.strip_prefix(k)
                .filter(|_| k.ends_with(':'))
                .unwrap_or(rest),
        )
    });
    let style = if msg.contains("refused") || msg.contains("denied") || msg.contains("turned") {
        Style::new().fg(theme::BAD)
    } else if msg.contains("let ") || msg.contains("trusted") || msg.contains("paired") {
        Style::new().fg(theme::GOOD)
    } else {
        Style::new()
    };
    Line::from(vec![
        Span::styled(format!(" {time} "), theme::dim()),
        Span::styled(
            format!("{:<3}", daemon.trim_start_matches('#')),
            theme::dim(),
        ),
        Span::styled(format!("{glyph} "), theme::accent()),
        Span::styled(msg.trim().to_string(), style),
    ])
}
