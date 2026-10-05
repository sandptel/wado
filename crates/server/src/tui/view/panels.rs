//! The five lists down the left. Each row is one line; the selected row of the focused panel
//! is highlighted, and the others keep their cursor so switching back lands where you were.

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Gauge, List, ListItem, ListState},
};

use super::{block, panel_title};
use crate::tui::app::{App, Panel, SETTINGS, Setting};
use crate::tui::theme;

pub fn draw(f: &mut Frame, app: &App, area: Rect) {
    let h = |p: Panel| app.len(p).max(1) as u16 + 2;
    let areas = Layout::vertical([
        Constraint::Length(h(Panel::Rig) + 1),
        Constraint::Length(h(Panel::Sessions)),
        Constraint::Min(4),
        Constraint::Length(h(Panel::Pair)),
        Constraint::Length(h(Panel::Security)),
    ])
    .split(area);
    for (i, p) in Panel::ALL.iter().enumerate() {
        let rows = match p {
            Panel::Rig => rig(app),
            Panel::Sessions => sessions(app),
            Panel::Devices => devices(app),
            Panel::Pair => vec![Line::from(vec![
                Span::styled(" + ", theme::key()),
                Span::raw("new QR code / link"),
            ])],
            Panel::Security => security(app),
        };
        list(f, app, *p, rows, areas[i]);
    }
    // The pool's occupancy under the Rig list: the one number that says whether the next
    // device will get in.
    let rig = areas[0];
    if rig.height < h(Panel::Rig) + 1 {
        return;
    }
    let busy = app.snap.status.sessions.len();
    let total = app.snap.status.daemons.len().max(1);
    let gauge = Gauge::default()
        .gauge_style(Style::new().fg(if busy >= total {
            theme::WARN
        } else {
            theme::ACCENT
        }))
        .ratio((busy as f64 / total as f64).min(1.0))
        .label(format!(
            "pool {busy}/{} busy",
            app.snap.status.daemons.len()
        ));
    let g = Rect {
        x: rig.x + 2,
        y: rig.y + rig.height.saturating_sub(2),
        width: rig.width.saturating_sub(4),
        height: 1,
    };
    f.render_widget(gauge, g);
}

fn list(f: &mut Frame, app: &App, p: Panel, rows: Vec<Line<'static>>, area: Rect) {
    let focused = app.focus == p;
    let empty = rows.is_empty();
    let items: Vec<ListItem> = if empty {
        vec![ListItem::new(Line::styled(" none", theme::dim()))]
    } else {
        rows.into_iter().map(ListItem::new).collect()
    };
    let mut state = ListState::default();
    if focused && !empty {
        state.select(Some(app.at(p)));
    }
    let l = List::new(items)
        .block(block(panel_title(p, focused), focused))
        .highlight_style(theme::selected());
    f.render_stateful_widget(l, area, &mut state);
}

fn rig(app: &App) -> Vec<Line<'static>> {
    let s = &app.snap.status;
    let row = |up: bool, name: String, note: String| {
        let (g, st) = theme::dot(up);
        Line::from(vec![
            Span::styled(format!(" {g} "), st),
            Span::raw(format!("{name:<10}")),
            Span::styled(note, theme::dim()),
        ])
    };
    let mut out = vec![
        row(s.relay.is_some(), "relay".into(), s.relay_addr.clone()),
        row(
            s.tunnel.is_some(),
            "tunnel".into(),
            s.tunnel
                .as_deref()
                .map(|t| t.trim_start_matches("https://").to_string())
                .unwrap_or_else(|| "none".into()),
        ),
    ];
    for d in &s.daemons {
        let note = match app.snap.session_on(d) {
            Some(sess) => format!(
                "{} {}",
                theme::PLAY,
                sess.viewer.as_deref().unwrap_or("detached")
            ),
            None => "idle".into(),
        };
        out.push(row(true, format!("daemon {d}"), note));
    }
    out
}

fn sessions(app: &App) -> Vec<Line<'static>> {
    app.snap
        .status
        .sessions
        .iter()
        .map(|s| {
            let who = s.viewer.clone().unwrap_or_else(|| "detached".into());
            Line::from(vec![
                Span::styled(
                    format!(
                        " {} ",
                        if s.viewer.is_some() {
                            theme::PLAY
                        } else {
                            "⏸"
                        }
                    ),
                    Style::new().fg(theme::GOOD),
                ),
                Span::raw(format!("{who:<14.14}")),
                Span::styled(
                    format!(" {}×{} {}", s.width, s.height, uptime(s.age_s)),
                    theme::dim(),
                ),
            ])
        })
        .collect()
}

fn devices(app: &App) -> Vec<Line<'static>> {
    let twins = |name: &str| {
        app.snap
            .devices
            .iter()
            .filter(|r| r.device.name == name)
            .count()
            > 1
    };
    app.snap
        .devices
        .iter()
        .map(|r| {
            // Two "Android · Chrome"s are told apart by the start of their key.
            let name = if twins(&r.device.name) {
                format!("{:.15} ·{:.4}", r.device.name, r.device.key)
            } else {
                r.device.name.clone()
            };
            let (g, st) = theme::dot(r.online);
            let files = match r.grants.files {
                "none" => Span::styled("  –", theme::dim()),
                l => Span::styled(format!(" {l:>2}"), Style::new().fg(theme::ACCENT)),
            };
            Line::from(vec![
                Span::styled(format!(" {g} "), st),
                Span::raw(format!("{name:<20.20}")),
                files,
                Span::styled(
                    format!(
                        " {}",
                        if r.device.pinned {
                            theme::PINNED
                        } else {
                            theme::UNPINNED
                        }
                    ),
                    if r.device.pinned {
                        Style::new().fg(theme::GOOD)
                    } else {
                        theme::dim()
                    },
                ),
                Span::styled(
                    if r.owner {
                        format!(" {}", theme::OWNER)
                    } else {
                        String::new()
                    },
                    Style::new().fg(theme::WARN),
                ),
            ])
        })
        .collect()
}

fn security(app: &App) -> Vec<Line<'static>> {
    let c = &app.snap.cfg;
    SETTINGS
        .iter()
        .map(|s| {
            let (name, value, warn) = match s {
                Setting::Join => ("join", c.security.join.clone(), c.security.open_join()),
                Setting::TrustFirst => (
                    "first device",
                    if c.security.trust_first_device {
                        "trusted".into()
                    } else {
                        "asks".into()
                    },
                    false,
                ),
                Setting::NewDevice => (
                    "new device",
                    crate::cli::devices::describe(&crate::gate::Grants::new_device()),
                    false,
                ),
                Setting::Files => ("file manager", on_off(c.files.enabled), false),
                Setting::Shells => ("shells", on_off(c.shells.enabled), false),
            };
            Line::from(vec![
                Span::raw(format!(" {name:<13}")),
                Span::styled(
                    value,
                    if warn {
                        Style::new().fg(theme::WARN)
                    } else {
                        theme::accent()
                    },
                ),
            ])
        })
        .collect()
}

fn on_off(b: bool) -> String {
    if b { "on".into() } else { "off".into() }
}

pub fn uptime(s: u64) -> String {
    match s {
        0..60 => format!("{s}s"),
        60..3600 => format!("{}m", s / 60),
        _ => format!("{}h{:02}m", s / 3600, s % 3600 / 60),
    }
}
