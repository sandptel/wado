//! Drawing. A dashboard, not a list-and-detail: everything that matters is on screen at once.
//!
//! ```text
//!   ╻ ╻┏━┓╺┳┓┏━┓  big Remote ID                       relay · tunnel · pool · version
//!   ┌ Pair ──────┐┌ Screens ─────────────────┐┌ Load ────────┐
//!   │  QR code   ││ each daemon, its session ││ braille CPU  │
//!   │            │└──────────────────────────┘└──────────────┘
//!   │            │┌ Devices ─────────────────────────────────┐
//!   │ checklist  ││ device × files shells settings host owner│
//!   │ link       │└──────────────────────────────────────────┘
//!   │            │┌ Activity ────────────────┐┌ Switches ────┐
//!   └────────────┘└──────────────────────────┘└──────────────┘
//!   keys for the card in focus, or the last action's outcome
//! ```

mod activity;
mod devices;
mod header;
mod load;
mod pair;
mod qr;
mod screens;
mod switches;

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, BorderType, Paragraph},
};

use super::app::{App, Card, Target};
use super::theme;

/// Below this the Pair card and the device matrix cannot both fit.
const MIN: (u16, u16) = (110, 28);

pub fn draw(f: &mut Frame, app: &App) {
    app.hits.borrow_mut().clear();
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
        Constraint::Length(3),
        Constraint::Min(10),
        Constraint::Length(1),
    ])
    .areas(a);
    header::draw(f, app, head);

    let [left, right] = Layout::horizontal([
        Constraint::Length(pair::width(app, body.height)),
        Constraint::Min(50),
    ])
    .areas(body);
    pair::draw(f, app, left);

    // Fixed rows hug their content; the screens and the load graphs take what is left, so a
    // taller terminal draws bigger screens rather than empty boxes.
    let wide = right.width >= 88;
    let dev_h = app.snap.devices.len().max(1) as u16 + 3;
    let bottom_h = if wide { 10 } else { 16 };
    let [top, mid, bottom] = Layout::vertical([
        Constraint::Min(8),
        Constraint::Length(dev_h),
        Constraint::Length(bottom_h),
    ])
    .areas(right);
    if wide {
        let [s, l] = Layout::horizontal([Constraint::Min(40), Constraint::Length(36)]).areas(top);
        screens::draw(f, app, s);
        load::draw(f, app, l);
    } else {
        screens::draw(f, app, top);
    }
    devices::draw(f, app, mid);
    let [act, sw] = if wide {
        Layout::horizontal([Constraint::Min(30), Constraint::Length(46)]).areas(bottom)
    } else {
        let [s, a] = Layout::vertical([Constraint::Length(8), Constraint::Min(3)]).areas(bottom);
        [a, s]
    };
    activity::draw(f, app, act);
    switches::draw(f, app, sw);

    f.render_widget(footer(app), foot);
}

/// A rounded card. The focused one is accented, its title bold.
pub fn card<'a>(title: &str, focused: bool) -> Block<'a> {
    Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(theme::border(focused))
        .title(Span::styled(
            format!(" {title} "),
            if focused { theme::key() } else { theme::bold() },
        ))
}

/// Draw `spans` left to right from `(x, y)`, registering each span whose target is set as
/// clickable. Returns the x after the last span.
pub fn row(
    f: &mut Frame,
    app: &App,
    mut x: u16,
    y: u16,
    max_x: u16,
    spans: Vec<(Span<'static>, Option<Target>)>,
) -> u16 {
    for (s, t) in spans {
        let w = (s.width() as u16).min(max_x.saturating_sub(x));
        if w == 0 {
            break;
        }
        let r = Rect::new(x, y, w, 1);
        f.render_widget(s, r);
        if let Some(t) = t {
            app.hits.borrow_mut().push((r, t));
        }
        x += w;
    }
    x
}

/// One toggle cell: `label` padded to `width`, green when on, the cursor's colour under it.
pub fn toggle(label: String, on: bool, cursor: bool, width: usize) -> Span<'static> {
    let style = if cursor {
        theme::cursor()
    } else {
        theme::state(on)
    };
    Span::styled(format!("{label:^width$}"), style)
}

fn footer(app: &App) -> Paragraph<'static> {
    if let Some((_, name)) = &app.confirm {
        return Paragraph::new(Line::from(vec![
            Span::styled(format!(" Unpair {name}? "), Style::new().fg(theme::BAD)),
            Span::styled(" y ", theme::cursor()),
            Span::styled(" unpair   any other key keeps it", theme::dim()),
        ]));
    }
    let mut spans = vec![Span::raw(" ")];
    let hints: &[(&str, &str)] = match app.focus {
        _ if !app.snap.asking.is_empty() => &[("y", "the words match"), ("n", "they don't")],
        Card::Pair => &[
            ("space", "allow/deny"),
            ("n", "new code"),
            ("c", "copy link"),
            ("z", "QR size"),
        ],
        Card::Devices => &[("space", "flip"), ("x", "unpair")],
        Card::Switches => &[("space", "flip")],
    };
    for (k, what) in hints
        .iter()
        .chain(&[("arrows", "move · across cards"), ("q", "quit")])
    {
        spans.push(Span::styled(k.to_string(), theme::key()));
        spans.push(Span::styled(format!(" {what}  "), theme::dim()));
    }
    spans.push(Span::styled("· click works too", theme::dim()));
    if let Some((o, _)) = &app.toast {
        let (text, color) = match o {
            Ok(m) => (format!("   {} {m}", theme::ON), theme::GOOD),
            Err(e) => (format!("   ✗ {e}"), theme::BAD),
        };
        spans.push(Span::styled(text, Style::new().fg(color)));
    }
    Paragraph::new(Line::from(spans))
}
