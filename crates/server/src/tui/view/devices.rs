//! The Devices card: one matrix — a row per trusted device, a column per grant, then owner.
//! Every cell is a toggle; the cursor is a cell, not a row.

use ratatui::{Frame, layout::Rect, style::Style, text::Span};

use super::{card, row, toggle};
use crate::tui::app::{App, Card, OWNER_COL, Target};
use crate::tui::theme;

/// `(wide label, narrow label)` per column, grants then owner.
const COLS: [(&str, &str); 5] = [
    ("files", "fil"),
    ("shells", "sh"),
    ("settings", "set"),
    ("host", "hst"),
    ("owner", theme::OWNER),
];
const NOW: usize = 8;

pub fn draw(f: &mut Frame, app: &App, area: Rect) {
    let focused = app.focus == Card::Devices;
    let block = card(&format!("Devices {}", app.snap.devices.len()), focused);
    let inner = block.inner(area);
    f.render_widget(block, area);
    let rows = &app.snap.devices;
    if rows.is_empty() {
        f.render_widget(
            Span::styled(
                " nobody yet — scan the code on the left with a phone",
                theme::dim(),
            ),
            inner,
        );
        return;
    }
    let wide = inner.width >= 74;
    let cw = if wide { 10 } else { 5 };
    let name_w = (inner.width as usize)
        .saturating_sub(NOW + cw * COLS.len() + 1)
        .max(12);

    let mut head = vec![
        (
            Span::styled(format!("   {:<w$}", "device", w = name_w - 3), theme::dim()),
            None,
        ),
        (Span::styled(format!("{:<NOW$}", "now"), theme::dim()), None),
    ];
    for (w, n) in COLS {
        head.push((
            Span::styled(format!("{:^cw$}", if wide { w } else { n }), theme::dim()),
            None,
        ));
    }
    row(f, app, inner.x, inner.y, inner.right(), head);

    let twins = |name: &str| rows.iter().filter(|r| r.device.name == name).count() > 1;
    let cur = app.at(Card::Devices);
    for (i, r) in rows.iter().enumerate() {
        let y = inner.y + 1 + i as u16;
        if y >= inner.bottom() {
            break;
        }
        let d = &r.device;
        // Two "Android · Chrome"s are told apart by the start of their key.
        let name = if twins(&d.name) {
            format!("{} ·{:.4}", d.name, d.key)
        } else {
            d.name.clone()
        };
        let pin = if d.pinned {
            theme::PINNED
        } else {
            theme::UNPINNED
        };
        let selected = cur.is_some_and(|(cr, _)| cr == i);
        let name_style = if selected {
            theme::selected()
        } else {
            Style::new()
        };
        let (dot, dot_st) = theme::dot(r.online);
        let mut spans = vec![
            (
                Span::styled(format!(" {pin} "), theme::state(d.pinned)),
                None,
            ),
            (
                Span::styled(
                    format!("{:<w$.w$}", name, w = name_w.saturating_sub(3)),
                    name_style,
                ),
                None,
            ),
            (
                Span::styled(
                    format!(
                        "{dot} {:<w$}",
                        if r.online { "live" } else { "" },
                        w = NOW - 2
                    ),
                    dot_st,
                ),
                None,
            ),
        ];
        let g = &r.grants;
        for c in 0..COLS.len() {
            // The owner may already do all but files; those cells show it, greyed.
            let implied = r.owner && (1..OWNER_COL).contains(&c);
            let (label, on) = match c {
                0 => match g.files {
                    // Files count only on a device paired by QR.
                    l @ ("ro" | "rw") if !d.pinned => (format!("{l}!"), false),
                    l @ ("ro" | "rw") if !wide => (l.into(), true),
                    "ro" => ("read".into(), true),
                    "rw" => ("write".into(), true),
                    _ => (theme::OFF.into(), false),
                },
                1 => mark(g.shells),
                2 => mark(g.settings),
                3 => mark(g.host),
                _ => (
                    if r.owner { theme::OWNER } else { theme::OFF }.into(),
                    r.owner,
                ),
            };
            let at = cur == Some((i, c));
            let mut s = toggle(label, on, at, cw);
            if implied && !at {
                s = s.style(Style::new().fg(theme::WARN));
            }
            if c == OWNER_COL && r.owner && !at {
                s = s.style(Style::new().fg(theme::WARN));
            }
            spans.push((s, Some(Target::Cell(Card::Devices, i, c))));
        }
        row(f, app, inner.x, y, inner.right(), spans);
    }
}

fn mark(on: bool) -> (String, bool) {
    (if on { theme::ON } else { theme::OFF }.into(), on)
}
