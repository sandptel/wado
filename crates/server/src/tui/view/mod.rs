//! Drawing. One frame, top to bottom:
//!
//! ```text
//!   header    id · relay · tunnel · daemons · join policy · version
//!   ┌ panels ─┐┌ detail of the selected row ───────────┐
//!   │1 Rig    ││                                        │
//!   │2 Sess.  ││                                        │
//!   │3 Devices│├ activity ──────────────────────────────┤
//!   │4 Pair   ││ gate / files / shells / config lines   │
//!   │5 Secur. ││                                        │
//!   footer    keys for this panel, or the last action's outcome
//! ```
//!
//! - [`panels`] — the five lists on the left.
//! - [`detail`] — the right-hand pane for each.
//! - [`overlay`] — the approval prompt, confirmations, help.

mod detail;
mod overlay;
mod panels;

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph},
};

use super::app::{App, Panel};
use super::theme;

/// Below this the five panels cannot each show a row.
const MIN: (u16, u16) = (90, 30);

pub fn draw(f: &mut Frame, app: &App) {
    let a = f.area();
    if a.width < MIN.0 || a.height < MIN.1 {
        let msg = format!(
            "wado needs {}×{} — this terminal is {}×{}",
            MIN.0, MIN.1, a.width, a.height
        );
        f.render_widget(Paragraph::new(Line::styled(msg, theme::dim())), a);
        return;
    }
    let [head, body, foot] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(8),
        Constraint::Length(1),
    ])
    .areas(f.area());
    f.render_widget(header(app), head);

    let [left, right] =
        Layout::horizontal([Constraint::Length(36), Constraint::Min(30)]).areas(body);
    panels::draw(f, app, left);

    let [det, act] =
        Layout::vertical([Constraint::Min(18), Constraint::Percentage(30)]).areas(right);
    detail::draw(f, app, det);
    activity(f, app, act);

    f.render_widget(footer(app), foot);
    overlay::draw(f, app);
}

/// A rounded block titled `[n] Title`, accented when focused.
pub fn block<'a>(title: impl Into<Line<'a>>, focused: bool) -> Block<'a> {
    Block::new()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::border(focused))
        .title(title)
}

pub fn panel_title(p: Panel, focused: bool) -> Line<'static> {
    let n = Panel::ALL.iter().position(|x| *x == p).unwrap_or(0) + 1;
    Line::from(vec![
        Span::styled(format!("[{n}]"), theme::key()),
        Span::styled(
            format!(" {} ", p.title()),
            if focused { theme::bold() } else { Style::new() },
        ),
    ])
}

fn header(app: &App) -> Paragraph<'static> {
    let s = &app.snap.status;
    let mut spans = vec![
        Span::styled(" ▌wado ", theme::key()),
        Span::styled(
            s.remote_id.clone().unwrap_or_else(|| "no id yet".into()),
            theme::bold(),
        ),
        Span::raw("   "),
    ];
    let mut pill = |up: bool, label: String| {
        let (g, st) = theme::dot(up);
        spans.push(Span::styled(g, st));
        spans.push(Span::raw(format!(" {label}   ")));
    };
    pill(s.relay.is_some(), "relay".into());
    pill(s.tunnel.is_some(), "tunnel".into());
    pill(
        !s.daemons.is_empty(),
        format!(
            "{} daemon{}",
            s.daemons.len(),
            if s.daemons.len() == 1 { "" } else { "s" }
        ),
    );
    if app.snap.cfg.security.open_join() {
        spans.push(Span::styled(
            format!("{} join open", theme::ALERT),
            Style::new().fg(theme::WARN),
        ));
        spans.push(Span::raw("   "));
    }
    if !app.snap.pending.is_empty() {
        spans.push(Span::styled(
            format!("{} {} waiting", theme::WAIT, app.snap.pending.len()),
            Style::new().fg(theme::WARN),
        ));
    }
    spans.push(Span::styled(
        format!("   {}", crate::cli::landing::VERSION),
        theme::dim(),
    ));
    Paragraph::new(Line::from(spans))
}

fn activity(f: &mut Frame, app: &App, area: Rect) {
    let inner_h = area.height.saturating_sub(2) as usize;
    let lines: Vec<Line> = app
        .snap
        .activity
        .iter()
        .rev()
        .take(inner_h)
        .rev()
        .map(|l| {
            let (time, rest) = l.split_at(l.find(' ').unwrap_or(0));
            let style = if rest.contains("refused") || rest.contains("denied") {
                Style::new().fg(theme::BAD)
            } else if rest.contains("let ") || rest.contains("trusted") {
                Style::new().fg(theme::GOOD)
            } else {
                Style::new()
            };
            Line::from(vec![
                Span::styled(time.to_string(), theme::dim()),
                Span::styled(rest.to_string(), style),
            ])
        })
        .collect();
    let empty = lines.is_empty();
    let p = Paragraph::new(if empty {
        vec![Line::styled(
            " nothing yet — joins, grants and file activity show here",
            theme::dim(),
        )]
    } else {
        lines
    })
    .block(block(" Activity ", false));
    f.render_widget(p, area);
}

fn footer(app: &App) -> Paragraph<'static> {
    if let Some((o, _)) = &app.toast {
        let (text, color) = match o {
            Ok(m) => (format!(" {} {m}", theme::ON), theme::GOOD),
            Err(e) => (format!(" ✗ {e}"), theme::BAD),
        };
        return Paragraph::new(Line::styled(text, Style::new().fg(color)));
    }
    let hints: &[(&str, &str)] = match app.focus {
        Panel::Devices => &[
            ("f", "files"),
            ("s", "shells"),
            ("c", "settings"),
            ("h", "host"),
            ("o", "make owner"),
            ("u", "unpair"),
        ],
        Panel::Pair => &[("f s c h", "checklist"), ("⏎", "make QR")],
        Panel::Security => &[("⏎", "toggle"), ("f s c h", "new-device grants")],
        _ => &[],
    };
    let mut spans = vec![Span::raw(" ")];
    for (k, what) in hints.iter().chain(&[
        ("1-5", "panel"),
        ("j/k", "move"),
        ("?", "help"),
        ("q", "quit"),
    ]) {
        spans.push(Span::styled(k.to_string(), theme::key()));
        spans.push(Span::styled(format!(" {what}  "), theme::dim()));
    }
    Paragraph::new(Line::from(spans))
}
