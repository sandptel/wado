//! What floats over the panel: a device asking to come in, a confirmation, the help card.

use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Clear, Paragraph},
};

use super::block;
use crate::tui::app::{App, Modal};
use crate::tui::theme;

pub fn draw(f: &mut Frame, app: &App) {
    match &app.modal {
        Some(Modal::Help) => return help(f),
        Some(Modal::Unpair { name, .. }) => {
            return card(
                f,
                " Unpair ",
                vec![
                    Line::raw(format!(" Forget {name}?")),
                    Line::styled(
                        " It will have to be approved or paired again.",
                        theme::dim(),
                    ),
                    Line::raw(""),
                    keys(&[("y", "unpair"), ("any key", "keep it")]),
                ],
                Style::new().fg(theme::BAD),
            );
        }
        None => {}
    }
    if let Some(r) = app.asking() {
        let more = app.snap.pending.len().saturating_sub(1);
        let mut l = vec![
            Line::from(vec![
                Span::raw(" "),
                Span::styled(r.name.clone(), theme::bold()),
                Span::raw(" wants to use this computer"),
            ]),
            Line::styled(format!(" from {}", r.addr), theme::dim()),
            Line::raw(""),
            keys(&[
                ("o", "once"),
                ("a", "always"),
                ("d", "deny"),
                ("esc", "later"),
            ]),
        ];
        if more > 0 {
            l.push(Line::styled(format!(" {more} more waiting"), theme::dim()));
        }
        card(
            f,
            &format!(" {} Knock knock ", theme::WAIT),
            l,
            Style::new().fg(theme::WARN),
        );
    }
}

fn keys(k: &[(&str, &str)]) -> Line<'static> {
    let mut spans = vec![Span::raw(" ")];
    for (key, what) in k {
        spans.push(Span::styled(key.to_string(), theme::key()));
        spans.push(Span::styled(format!(" {what}   "), theme::dim()));
    }
    Line::from(spans)
}

fn card(f: &mut Frame, title: &str, lines: Vec<Line<'static>>, border: Style) {
    let area = centered(f.area(), 58, lines.len() as u16 + 2);
    f.render_widget(Clear, area);
    f.render_widget(
        Paragraph::new(lines).block(block(title.to_string(), true).border_style(border)),
        area,
    );
}

fn centered(area: Rect, w: u16, h: u16) -> Rect {
    let [r] = Layout::vertical([Constraint::Length(h)])
        .flex(Flex::Center)
        .areas(area);
    let [c] = Layout::horizontal([Constraint::Length(w.min(area.width))])
        .flex(Flex::Center)
        .areas(r);
    c
}

fn help(f: &mut Frame) {
    let section = |t: &str| Line::styled(format!(" {t}"), theme::bold());
    let row = |k: &str, what: &str| {
        Line::from(vec![
            Span::styled(format!("   {k:<10}"), theme::key()),
            Span::raw(what.to_string()),
        ])
    };
    let l = vec![
        section("Move"),
        row("1-5 tab", "jump to a panel"),
        row("j k ↑ ↓", "move in it"),
        Line::raw(""),
        section("Grants — on a device, the pairing checklist, or New device"),
        row("f", "files: none → read → read-write"),
        row("s", "shells"),
        row("c", "settings (privileged config, Wi-Fi)"),
        row("h", "host (sound, Bluetooth, sleep)"),
        Line::raw(""),
        section("Devices"),
        row("o", "make it the owner"),
        row("u", "unpair"),
        Line::raw(""),
        section("Elsewhere"),
        row("⏎ space", "make a pairing code · toggle a setting"),
        row("o a d", "answer a device waiting at the door"),
        row("r", "re-read now (it does so every second)"),
        row("q", "quit — the daemons keep running"),
        Line::raw(""),
        Line::styled(
            "   Everything here is also `wado <command>` — run `wado` to see.",
            theme::dim(),
        ),
    ];
    let area = centered(f.area(), 66, l.len() as u16 + 2);
    f.render_widget(Clear, area);
    f.render_widget(Paragraph::new(l).block(block(" Help ", true)), area);
}
