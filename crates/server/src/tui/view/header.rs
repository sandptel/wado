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
    // Where a phone reaches this computer: the rig's tunnel if one is up, else the relay's
    // public URL (`server { public-relay }`, or the dialled relay when it is not local).
    let public = s
        .tunnel
        .clone()
        .or_else(|| app.snap.cfg.server.public_url());
    let tunnel = pill(
        public.is_some(),
        "phones",
        public.as_deref().map_or(
            "no public relay — set server { public-relay }".into(),
            |t| t.trim_start_matches("https://").into(),
        ),
    );
    // A daemon that lost its relay says so, and says whose fault it is not (`crate::relay_watch`).
    let relay = match s.relay_lost_ms() {
        Some(ms) => pill(
            false,
            "relay",
            format!(
                "{} — lost {} ago, not your computer",
                s.relay_addr,
                crate::relay_watch::record::span(ms)
            ),
        ),
        None => pill(s.relay.is_some(), "relay", s.relay_addr.clone()),
    };
    let lines = vec![
        Line::from(
            relay
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
