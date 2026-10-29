// SPDX-License-Identifier: AGPL-3.0-only
//! The overview: who this computer is to the world, what is running, how the stream is made,
//! and what just happened. Plain words first; the technical name one line down.

use slint::ComponentHandle;

use super::Ctl;
use crate::{
    AppWindow, Host, LogLine, Stat,
    caps::{self, logs::Line, watch::HostSnapshot},
    models::sync,
};

pub fn wire(ui: &AppWindow) {
    let weak = ui.as_weak();
    ui.global::<Host>().on_start_daemon(move || {
        let Some(ui) = weak.upgrade() else { return };
        crate::toast::outcome(&ui, caps::daemon::start());
        super::with(|c| c.poke());
    });
}

fn stat(key: &str, value: impl Into<String>, sub: impl Into<String>) -> Stat {
    Stat {
        key: key.into(),
        value: value.into().into(),
        sub: sub.into().into(),
        tone: "dim".into(),
        motion: "still".into(),
    }
}

pub fn apply(ui: &AppWindow, ctl: &mut Ctl, s: &HostSnapshot) {
    let h = ui.global::<Host>();
    let p = &s.pool;
    let up = caps::daemon::presence(p).is_on();
    h.set_daemon_up(up);
    h.set_computer_name(s.hostname.as_str().into());
    h.set_remote_id(p.remote_id.clone().unwrap_or_default().into());

    let (words, tone, motion) = match (up, p.relay_up) {
        (true, true) => ("Online — reachable from anywhere", "good", "ripple"),
        (true, false) => (
            "Running, but the relay does not answer",
            "caution",
            "breathe",
        ),
        (false, _) => ("Not running — unreachable", "bad", "still"),
    };
    h.set_relay_words(words.into());
    h.set_relay_tone(tone.into());
    h.set_relay_motion(motion.into());

    let watching = s.devices.iter().filter(|d| d.online).count();
    sync(
        &ctl.models.meta,
        vec![
            stat(
                "DAEMONS",
                match p.daemons.len() {
                    0 => "None".to_string(),
                    1 => "1 running".to_string(),
                    n => format!("{n} running"),
                },
                match p.daemons.len() {
                    0 => "start one to be reachable".to_string(),
                    1 => "one device at a time".to_string(),
                    n => format!("{n} devices at once"),
                },
            ),
            stat(
                "RELAY",
                if p.relay_up { "Connected" } else { "No answer" },
                match p.pooled {
                    Some(n) => format!("{} · sees {n}", p.relay_addr),
                    None => p.relay_addr.clone(),
                },
            ),
            stat(
                "DEVICES",
                format!("{} paired", s.devices.len()),
                if watching == 0 {
                    "none connected now".to_string()
                } else {
                    format!("{watching} connected now")
                },
            ),
        ],
    );

    // Live: the first session (newest first), and the encoder as the log last told it.
    let first = s.sessions.first();
    let link = s.session_links.first().cloned().flatten();
    let (lw, ..) =
        caps::devices::link_words(link.as_ref(), first.is_some_and(|x| x.viewer.is_some()));
    sync(
        &ctl.models.live,
        vec![
            stat(
                "SESSIONS",
                s.sessions.len().to_string(),
                first
                    .and_then(|x| x.apps.first().cloned())
                    .unwrap_or_else(|| "idle".into()),
            ),
            stat(
                "SHAPE",
                first.map_or("—".into(), |x| format!("{}×{}", x.width, x.height)),
                first.map_or("no session".into(), |x| format!("{} fps", x.fps)),
            ),
            stat(
                "LINK",
                if first.is_some() { lw } else { "—".into() },
                first
                    .and_then(|x| x.viewer.clone())
                    .unwrap_or_else(|| "nobody watching".into()),
            ),
        ],
    );
    let e = &s.encoder;
    let (ew, et) = match (e.pipeline.as_deref(), e.mode.as_deref()) {
        (Some("vaapi-dmabuf"), _) => ("Hardware encode · zero-copy", "good"),
        (Some("vaapi-cpu"), _) => ("Hardware encode · CPU copy", "good"),
        (Some(p), _) if p.starts_with("x264") => ("Software encode", "warn"),
        (None, Some("software")) => ("Software encode", "warn"),
        (None, Some("hardware")) => ("Hardware encode", "good"),
        _ => ("", "dim"),
    };
    h.set_encoder_words(ew.into());
    h.set_encoder_tone(et.into());
    h.set_encoder_detail(
        if ew.is_empty() {
            "No session has started since the log began, so there is no encoder to name."
                .to_string()
        } else {
            let tier = e.pipeline.clone().or(e.mode.clone()).unwrap_or_default();
            let soft = et == "warn";
            format!(
                "Last seen in the daemon log at {} — {tier}.{}",
                e.at,
                if soft {
                    " The hardware encoder did not open, so video costs CPU and arrives later."
                } else {
                    ""
                }
            )
        }
        .into(),
    );

    let activity: Vec<LogLine> = s
        .logs
        .iter()
        .rev()
        .filter(|l| l.activity())
        .take(7)
        .map(row)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    sync(&ctl.models.activity, activity);
}

/// A log line as the UI shows it, toned by level.
pub fn row(l: &Line) -> LogLine {
    let tone = match l.level.as_str() {
        "ERROR" => "bad",
        "WARN" => "warn",
        "INFO" => "good",
        "DEBUG" => "accent",
        _ => "dim",
    };
    LogLine {
        time: l.time.as_str().into(),
        level: l.level.as_str().into(),
        tone: tone.into(),
        target: l.target.as_str().into(),
        msg: l.msg.as_str().into(),
    }
}
