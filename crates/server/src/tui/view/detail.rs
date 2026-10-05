//! The right-hand pane: everything about the selected row, and — for a device or a pairing code
//! — the checklist of what it may do, each line with the key that flips it.

use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
};

use super::block;
use crate::gate::Grants;
use crate::tui::app::{App, Panel, SETTINGS, Setting};
use crate::tui::theme;

pub fn draw(f: &mut Frame, app: &App, area: Rect) {
    let (title, lines) = match app.focus {
        Panel::Rig => rig(app),
        Panel::Sessions => session(app),
        Panel::Devices => device(app),
        Panel::Pair => return pair(f, app, area),
        Panel::Security => security(app),
    };
    let p = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .block(block(format!(" {title} "), false));
    f.render_widget(p, area);
}

fn kv(k: &str, v: impl Into<String>) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!(" {k:<12}"), theme::dim()),
        Span::raw(v.into()),
    ])
}

fn note(s: impl Into<String>) -> Line<'static> {
    Line::styled(format!(" {}", s.into()), theme::dim())
}

fn rig(app: &App) -> (String, Vec<Line<'static>>) {
    let s = &app.snap.status;
    match app.at(Panel::Rig) {
        0 => (
            "Relay".into(),
            vec![
                kv("address", s.relay_addr.clone()),
                kv(
                    "state",
                    if s.relay.is_some() {
                        "up"
                    } else {
                        "not answering"
                    },
                ),
                kv(
                    "pool",
                    s.pooled()
                        .map_or("?".into(), |n| format!("{n} daemons registered")),
                ),
                kv("health", s.relay.clone().unwrap_or_default()),
                Line::raw(""),
                note("Where phones and this computer find each other. It routes the"),
                note("handshake and sealed control messages; video never passes through."),
            ],
        ),
        1 => (
            "Tunnel".into(),
            vec![
                kv("url", s.tunnel.clone().unwrap_or_else(|| "none".into())),
                Line::raw(""),
                note("The rig's public way in to the relay (cloudflared). It is reused across"),
                note("rig restarts; only scripts/rig.sh --stop rotates it."),
            ],
        ),
        i => {
            let inst = s.daemons.get(i - 2).cloned().unwrap_or_default();
            let mut l = vec![kv("instance", inst.clone())];
            match app.snap.session_on(&inst) {
                Some(sess) => {
                    l.push(kv(
                        "session",
                        format!(
                            "{}×{} @ {} fps, up {}",
                            sess.width,
                            sess.height,
                            sess.fps,
                            super::panels::uptime(sess.age_s)
                        ),
                    ));
                    l.push(kv(
                        "watching",
                        sess.viewer.clone().unwrap_or_else(|| "nobody".into()),
                    ));
                }
                None => l.push(kv("session", "idle — the next device lands here")),
            }
            l.push(kv(
                "log",
                crate::cli::status::rig_dir()
                    .join(format!("daemon-{inst}.log"))
                    .display()
                    .to_string(),
            ));
            (format!("Daemon {inst}"), l)
        }
    }
}

fn session(app: &App) -> (String, Vec<Line<'static>>) {
    let Some(s) = app.snap.status.sessions.get(app.at(Panel::Sessions)) else {
        return (
            "Session".into(),
            vec![note(
                "No session running. One starts when a device connects.",
            )],
        );
    };
    let mut l = vec![
        kv(
            "watching",
            s.viewer
                .clone()
                .unwrap_or_else(|| "nobody (detached)".into()),
        ),
        kv("daemon", s.instance.clone()),
        kv("size", format!("{}×{} @ {} fps", s.width, s.height, s.fps)),
        kv("up", super::panels::uptime(s.age_s)),
        Line::raw(""),
        Line::styled(" Windows", theme::bold()),
    ];
    if s.apps.is_empty() {
        l.push(note("  none open"));
    }
    l.extend(s.apps.iter().map(|a| Line::raw(format!("   {a}"))));
    ("Session".into(), l)
}

fn device(app: &App) -> (String, Vec<Line<'static>>) {
    let Some(r) = app.snap.devices.get(app.at(Panel::Devices)) else {
        return (
            "Device".into(),
            vec![note("No trusted devices yet. Press 4 to pair one.")],
        );
    };
    let d = &r.device;
    let mut l = vec![
        kv("key", d.key.clone()),
        kv(
            "paired",
            if d.pinned {
                "by QR code — its key is pinned"
            } else {
                "on first use or by approval — not pinned"
            },
        ),
        kv(
            "now",
            if r.online {
                "connected"
            } else {
                "not connected"
            },
        ),
    ];
    if r.owner {
        l.push(kv(
            "role",
            "owner — may do everything except files without a grant",
        ));
    }
    l.push(Line::raw(""));
    l.extend(checklist(&r.grants, d.pinned, r.owner));
    ("Device · ".to_string() + &d.name, l)
}

/// The grants as a checklist, each with its key.
fn checklist(g: &Grants, pinned: bool, owner: bool) -> Vec<Line<'static>> {
    let row = |on: bool, name: &str, key: &str, why: &str| {
        let (mark, st) = if on {
            (theme::ON, Style::new().fg(theme::GOOD))
        } else {
            (theme::OFF, theme::dim())
        };
        Line::from(vec![
            Span::styled(format!("   {mark} "), st),
            Span::styled(
                format!("{name:<14}"),
                if on { theme::bold() } else { theme::dim() },
            ),
            Span::styled(format!("{key:<3}"), theme::key()),
            Span::styled(why.to_string(), theme::dim()),
        ])
    };
    let files = match g.files {
        "none" => "files".to_string(),
        l => format!("files · {l}"),
    };
    let files_why = if g.files != "none" && !pinned {
        "needs a QR pairing to count"
    } else {
        "browse (ro) or also upload, move, delete (rw)"
    };
    let locked = if owner { theme::OWNER } else { "" };
    vec![
        Line::styled(" Allowed", theme::bold()),
        row(
            true,
            "desktop",
            "",
            "see and drive it — every trusted device",
        ),
        row(g.files != "none", &files, "f", files_why),
        row(
            g.shells,
            "shells",
            if owner { locked } else { "s" },
            "the console's terminals and ssh",
        ),
        row(
            g.settings,
            "settings",
            if owner { locked } else { "c" },
            "privileged config, Wi-Fi",
        ),
        row(
            g.host,
            "host",
            if owner { locked } else { "h" },
            "sound, Bluetooth, sleep",
        ),
    ]
}

fn pair(f: &mut Frame, app: &App, area: Rect) {
    let mut l = vec![
        note("Whoever scans this is trusted and pinned, and may:"),
        Line::raw(""),
    ];
    l.extend(checklist(&app.pair_grants, true, false));
    l.push(Line::raw(""));
    match &app.pair_link {
        None => l.push(Line::from(vec![
            Span::styled("   ⏎ ", theme::key()),
            Span::raw("make the code"),
        ])),
        Some((link, made_with)) => {
            if *made_with != app.pair_grants {
                l.push(Line::styled(
                    "   checklist changed — ⏎ for a code that carries it",
                    Style::new().fg(theme::WARN),
                ));
            }
            match crate::cli::qr::render(link) {
                Some(q) if q.lines().count() as u16 + l.len() as u16 + 4 <= area.height => {
                    l.extend(q.lines().map(|q| Line::raw(format!("  {q}"))));
                }
                _ => l.push(note("(make the window taller to see the QR code)")),
            }
            l.push(Line::styled(format!(" {link}"), theme::accent()));
        }
    }
    let p = Paragraph::new(l)
        .wrap(Wrap { trim: false })
        .block(block(" Pair a device ", false));
    f.render_widget(p, area);
}

fn security(app: &App) -> (String, Vec<Line<'static>>) {
    let s = SETTINGS[app.at(Panel::Security)];
    let (title, text): (&str, &[&str]) = match s {
        Setting::Join => (
            "Join",
            &[
                "ask  — a device not on the trust list waits until a connected device",
                "       (or `wado approve`, or this panel) lets it in.",
                "open — any device that reaches this computer's id gets the desktop, once:",
                "       never trusted, no files, no shells. For testing only.",
            ],
        ),
        Setting::TrustFirst => (
            "First device",
            &[
                "While nobody is trusted yet, the first device to connect is trusted and",
                "becomes the owner. Off: the first device must be paired by QR too.",
            ],
        ),
        Setting::NewDevice => (
            "New device",
            &[
                "What a newly trusted device may do beyond the desktop — after an",
                "\"always\" approval, and as the starting checklist of a pairing code.",
                "",
                "f files · s shells · c settings · h host",
            ],
        ),
        Setting::Files => (
            "File manager",
            &["Off refuses every file request, whatever a device was granted."],
        ),
        Setting::Shells => (
            "Shells",
            &["Off refuses every shell, whatever a device was granted."],
        ),
    };
    let mut l: Vec<Line> = text.iter().map(|t| note(*t)).collect();
    l.push(Line::raw(""));
    l.push(note(
        "⏎ changes it. Written to ui.kdl and applied to every daemon now.",
    ));
    (title.into(), l)
}
