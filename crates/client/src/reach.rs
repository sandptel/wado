//! What this page knows about one computer right now, whichever way it knows it.
//!
//! Three sources, best first: the selected computer's own link (`live.link`, `live.hoststate`,
//! `live.host`), a light control-only link for a computer home has a widget for (`live.side`,
//! `js/side_link.js`), and the relay's online count for the rest (`live.online`). Cards, the
//! Computer page's hero and widgets all read a computer through [`of`], so they never disagree
//! about whether it is there.

use wado_protocol::{ConfigState, DeviceGrants, HostState};

use dioxus::prelude::*;

use crate::state::Ui;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Reach {
    pub selected: bool,
    /// `ok` | `warn` | `bad` | `wait` — the dot's colour, and whether it moves.
    pub tone: &'static str,
    /// The card's words: "Direct · 23 ms", "Online", "Offline · seen 2 h ago".
    pub words: String,
    /// A daemon is there to talk to.
    pub online: bool,
    /// Every seat is taken by another device.
    pub busy: bool,
    pub host: Option<HostState>,
    pub config: Option<ConfigState>,
    /// When it was last seen online, as words ("2 h ago"); empty if never.
    pub seen: String,
}

impl Reach {
    /// What this phone may do there. `None`: not known (an older daemon, or not asked yet) —
    /// offer everything and let the computer say no.
    pub fn grants(&self) -> Option<DeviceGrants> {
        self.config.as_ref().and_then(|c| c.grants.clone())
    }

    /// May this phone use `token` (`host`, `input`, `clipboard`, `shells`) here? Unknown counts
    /// as yes, for the reason above.
    pub fn may(&self, token: &str) -> bool {
        match self.grants() {
            None => true,
            Some(g) => match token {
                "host" => g.host,
                "input" => g.input,
                "clipboard" => g.clipboard,
                "shells" => g.shells,
                "settings" => g.settings,
                _ => true,
            },
        }
    }
}

pub fn of(ui: Ui, key: &str) -> Reach {
    let s = ui.set;
    let live = ui.live;
    let profiles = s.profiles.read();
    let cur = (s.profile)();
    let idx = profiles.iter().position(|p| p.key() == key);
    let seen = idx
        .and_then(|i| profiles.get(i))
        .map(|p| crate::profile::ago(p.seen))
        .unwrap_or_default();
    let offline_words = |seen: &str| {
        if seen.is_empty() {
            "Offline".to_string()
        } else {
            format!("Offline · seen {seen}")
        }
    };
    // With no profile yet, the settings themselves are the one computer.
    let selected = idx == Some(cur) || (profiles.is_empty() && key.is_empty());
    drop(profiles);

    if selected {
        let link = (live.link)();
        let stage = (live.conn_stage)();
        let err = (live.conn_error)();
        let host = (live.hoststate)();
        let config = (live.host_seen)().then(|| (live.host)());
        let online = host.is_some() || stage >= 2;
        let (tone, words) = match link.state.as_str() {
            _ if (live.session_on)() && stage >= 4 => ("ok", "Streaming".to_string()),
            "direct" => (
                "ok",
                link.rtt_ms
                    .map_or("Direct".into(), |ms| format!("Direct · {ms} ms")),
            ),
            "relayed" => ("warn", "Relayed · limited".into()),
            "connecting" | "reconnecting" => ("warn", "Connecting…".into()),
            "computer_offline" => ("bad", offline_words(&seen)),
            "waiting" if link.why.contains("in use") || err.contains("in use") => {
                ("warn", "In use on another device".into())
            }
            "waiting" => ("warn", "Waiting".into()),
            "ready" => ("ok", "Online".into()),
            _ if err.contains("in use") => ("warn", "In use on another device".into()),
            _ if online => ("ok", "Online".into()),
            "offline" if link.why == "no_network" => ("bad", "No network".into()),
            "offline" => ("bad", "Unreachable".into()),
            _ => ("wait", "Connecting…".into()),
        };
        // A stale state from before the daemon went is not "online".
        let online = online && tone != "bad";
        return Reach {
            selected: true,
            tone,
            busy: words.starts_with("In use"),
            words,
            online,
            host,
            config,
            seen,
        };
    }

    if let Some(side) = live.side.read().get(key).cloned() {
        let (tone, words, online) = match side.state.as_str() {
            "up" => ("ok", "Online".to_string(), true),
            "offline" => ("bad", offline_words(&seen), false),
            "busy" => ("warn", "In use on another device".into(), true),
            "waiting" => ("warn", "Waiting".into(), false),
            "refused" => ("bad", "Refused this phone".into(), false),
            "unreachable" => ("bad", "Relay unreachable".into(), false),
            _ => ("wait", "Connecting…".into(), false),
        };
        return Reach {
            selected: false,
            tone,
            busy: side.state == "busy",
            words,
            online,
            host: side.host,
            config: side.config,
            seen,
        };
    }

    let (tone, words, online) = match live.online.read().get(key).copied() {
        Some(n) if n > 0 => ("ok", "Online".to_string(), true),
        Some(0) => ("bad", offline_words(&seen), false),
        Some(_) => ("bad", "Relay unreachable".into(), false),
        None => ("wait", "Checking…".into(), false),
    };
    Reach {
        selected: false,
        tone,
        busy: false,
        words,
        online,
        host: None,
        config: None,
        seen,
    }
}
