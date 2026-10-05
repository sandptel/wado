//! Three rows across the top: the wordmark, the Remote ID big enough to read across a room, and
//! the rig's health.

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::tui::app::App;
use crate::tui::theme;

pub fn draw(f: &mut Frame, app: &App, area: Rect) {
    let s = &app.snap.status;
    // Groups split by space, not the dash: the big dash reads as a digit from across a room.
    let id = s
        .remote_id
        .clone()
        .unwrap_or_else(|| "--- --- ---".into())
        .replace('-', " ");
    let [mark, idc, rest] = Layout::horizontal([
        Constraint::Length(17),
        Constraint::Length(4 * id.len() as u16 + 12),
        Constraint::Min(20),
    ])
    .areas(area);

    let rows = |text: &str, style: Style| -> Vec<Line<'static>> {
        theme::big(text)
            .into_iter()
            .map(|r| Line::styled(r, style))
            .collect()
    };
    f.render_widget(
        Paragraph::new(rows("wado", theme::key())),
        Rect {
            x: mark.x + 1,
            ..mark
        },
    );
    let mut idl = rows(&id, theme::bold());
    for (i, l) in idl.iter_mut().enumerate() {
        l.spans.insert(
            0,
            Span::styled(
                ["remote ", "    id ", "       "][i].to_string(),
                theme::dim(),
            ),
        );
    }
    f.render_widget(Paragraph::new(idl), idc);

    let pooled = s.pooled().unwrap_or(0) as usize;
    let busy = s.sessions.len();
    let total = s.daemons.len();
    let meter: String = (0..total)
        .map(|i| if i < busy { '▰' } else { '▱' })
        .collect();
    let pill = |up: bool, name: &str, note: String| {
        let (g, st) = theme::dot(up);
        Line::from(vec![
            Span::styled(format!("{g} "), st),
            Span::styled(format!("{name:<7}"), theme::bold()),
            Span::styled(note, theme::dim()),
        ])
    };
    let mut pool = pill(
        total > 0,
        "pool",
        format!(" {busy}/{total} busy · {pooled} with the relay"),
    );
    pool.spans.insert(
        2,
        Span::styled(
            meter,
            Style::new().fg(if total > 0 && busy >= total {
                theme::WARN
            } else {
                theme::ACCENT
            }),
        ),
    );
    let mut tunnel = pill(
        s.tunnel.is_some(),
        "tunnel",
        s.tunnel
            .as_deref()
            .map_or("none".into(), |t| t.trim_start_matches("https://").into()),
    );
    if app.snap.cfg.security.open_join() {
        tunnel.spans.push(Span::styled(
            format!("   {} join is open", theme::ALERT),
            Style::new().fg(theme::WARN),
        ));
    }
    let lines = vec![
        Line::from(
            pill(s.relay.is_some(), "relay", s.relay_addr.clone())
                .spans
                .into_iter()
                .chain([Span::styled(
                    format!("   {}", crate::cli::landing::VERSION),
                    theme::dim(),
                )])
                .collect::<Vec<_>>(),
        ),
        tunnel,
        pool,
    ];
    f.render_widget(Paragraph::new(lines), rest);
}
