//! The Switches card: the few settings that change who gets in, each a slide toggle, and the
//! grants a newly trusted device starts with.

use ratatui::{Frame, layout::Rect, style::Style, text::Span};

use super::{card, row, toggle};
use crate::gate::Grants;
use crate::tui::app::{App, Card, GRANTS, SWITCHES, Switch, Target};
use crate::tui::theme;

pub fn draw(f: &mut Frame, app: &App, area: Rect) {
    let focused = app.focus == Card::Switches;
    let block = card("Switches", focused);
    let inner = block.inner(area);
    f.render_widget(block, area);
    let c = &app.snap.cfg;
    let cur = app.at(Card::Switches);
    let mut y = inner.y;
    for (i, s) in SWITCHES.iter().enumerate() {
        if y >= inner.bottom() {
            break;
        }
        let here = cur.is_some_and(|(r, _)| r == i);
        let label = |t: &str| {
            (
                Span::styled(
                    format!(" {t:<14}"),
                    if here {
                        theme::selected()
                    } else {
                        Style::new()
                    },
                ),
                Some(Target::Cell(Card::Switches, i, 0)),
            )
        };
        let (name, on, says, warn) = match s {
            Switch::Join => (
                "join",
                c.security.open_join(),
                if c.security.open_join() {
                    "anyone, once"
                } else {
                    "asks you first"
                },
                c.security.open_join(),
            ),
            Switch::TrustFirst => (
                "first device",
                c.security.trust_first_device,
                if c.security.trust_first_device {
                    "trusted on sight"
                } else {
                    "needs the QR"
                },
                false,
            ),
            Switch::Files => (
                "file manager",
                c.files.enabled,
                on_off(c.files.enabled),
                false,
            ),
            Switch::Shells => ("shells", c.shells.enabled, on_off(c.shells.enabled), false),
            Switch::NewDevice => {
                row(
                    f,
                    app,
                    inner.x,
                    y,
                    inner.right(),
                    vec![label("new devices")],
                );
                y += 1;
                let g = Grants::new_device();
                let mut spans = vec![(Span::raw("  "), None)];
                for (col, n) in GRANTS.iter().enumerate() {
                    let on = match col {
                        0 => g.files != "none",
                        1 => g.shells,
                        2 => g.settings,
                        _ => g.host,
                    };
                    let mark = if on { theme::ON } else { theme::OFF };
                    spans.push((
                        toggle(
                            format!("{mark} {n}"),
                            on,
                            cur == Some((i, col)),
                            n.len() + 4,
                        ),
                        Some(Target::Cell(Card::Switches, i, col)),
                    ));
                }
                if y < inner.bottom() {
                    row(f, app, inner.x, y, inner.right(), spans);
                }
                y += 1;
                continue;
            }
        };
        // A slide toggle: knob right and green when on.
        let knob = if on { "━━●" } else { "●━━" };
        let knob_style = match (here, on, warn) {
            (true, ..) => theme::cursor(),
            (_, true, true) => Style::new().fg(theme::WARN),
            (_, on, _) => theme::state(on),
        };
        row(
            f,
            app,
            inner.x,
            y,
            inner.right(),
            vec![
                label(name),
                (
                    Span::styled(knob, knob_style),
                    Some(Target::Cell(Card::Switches, i, 0)),
                ),
                (
                    Span::styled(
                        format!(" {says}"),
                        if warn {
                            Style::new().fg(theme::WARN)
                        } else {
                            theme::dim()
                        },
                    ),
                    None,
                ),
            ],
        );
        y += 1;
    }
}

fn on_off(b: bool) -> &'static str {
    if b { "on" } else { "off for everyone" }
}
