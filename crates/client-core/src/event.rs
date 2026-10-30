//! What the core tells its UI, and the view records in it. Pulled by the UI (`next_event`), never
//! pushed into it: the core never calls into the UI's runtime from its own threads.

use crate::feeds::host::HostView;
use crate::feeds::sessions::SessionRow;
use crate::link::state::{LinkState, Refusal};
use crate::pair::flow::PairStep;
use crate::profiles::Profile;
use crate::request::Outcome;

/// A saved computer, as its card shows it before (and while) its link reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComputerView {
    pub key: String,
    pub name: String,
    pub remote_id: String,
    pub relay_url: String,
    /// The OS line: the desktop, until the host state names the OS.
    pub os: String,
    pub logo: String,
    pub desktop: String,
    pub hue: u8,
}

impl ComputerView {
    pub fn of(p: &Profile) -> ComputerView {
        ComputerView {
            key: p.key.clone(),
            name: p.name.clone(),
            remote_id: p.remote_id.clone(),
            relay_url: p.relay_url.clone(),
            os: p.os_id.clone(),
            logo: crate::os_logo::slug(&p.os_id, &p.os_like).to_string(),
            desktop: p.desktop.clone(),
            hue: p.hue,
        }
    }
}

/// How the link words are drawn: a ripple, a breathe, still, or a warning.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Live,
    Busy,
    Still,
    Bad,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkView {
    pub state: LinkState,
    /// Plain words for the card.
    pub words: String,
    pub tone: Tone,
    /// The state's technical name, for Diagnostics.
    pub tech: String,
    /// The relay's caps, from its last `join_accepted`.
    pub caps: Vec<String>,
    /// The daemon of the computer's pool this link reached (`<remote id>:<n>`), or "" before the
    /// relay said. Together with the event's key (the computer), what anything per-host is
    /// labelled by — several computers' links run at once.
    pub instance: String,
}

fn ago(ms: u64) -> String {
    let s = ms / 1000;
    match s {
        0..=59 => "just now".into(),
        60..=3599 => format!("{} min ago", s / 60),
        3600..=86_399 => format!("{} h ago", s / 3600),
        _ => format!("{} d ago", s / 86_400),
    }
}

/// The words for `state`, `now_ms` on the link's clock.
pub fn words(state: &LinkState, now_ms: u64) -> (String, Tone) {
    match state {
        LinkState::Idle => ("Not connected".into(), Tone::Still),
        LinkState::Dialing | LinkState::Securing => ("Connecting…".into(), Tone::Busy),
        LinkState::Open { .. } => ("Online".into(), Tone::Live),
        LinkState::Reconnecting { in_ms, .. } => (
            format!("Reconnecting in {} s", in_ms.div_ceil(1000).max(1)),
            Tone::Busy,
        ),
        LinkState::Unreachable { .. } => ("Can't reach its relay".into(), Tone::Busy),
        LinkState::Offline { seen_ms: None } => ("Offline".into(), Tone::Still),
        LinkState::Offline { seen_ms: Some(t) } => (
            format!("Offline · seen {}", ago(now_ms.saturating_sub(*t))),
            Tone::Still,
        ),
        LinkState::Refused { why } => (
            match why {
                Refusal::PinMismatch => "Not this computer — re-pair",
                Refusal::NotTrusted => "Not paired with the app — pair again",
                Refusal::Revoked => "Removed on the computer — pair again",
            }
            .into(),
            Tone::Bad,
        ),
        LinkState::RelayTooOld => ("Relay too old for the app".into(), Tone::Bad),
    }
}

impl LinkView {
    pub fn new(state: LinkState, now_ms: u64, caps: Vec<String>, instance: String) -> LinkView {
        let (words, tone) = words(&state, now_ms);
        let tech = format!("{state:?}");
        LinkView {
            state,
            words,
            tone,
            tech,
            caps,
            instance,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum CoreEvent {
    Computers { list: Vec<ComputerView> },
    Link { key: String, view: LinkView },
    Host { key: String, view: HostView },
    Sessions { key: String, rows: Vec<SessionRow> },
    Pair { op: u64, step: PairStep },
    Answer { op: u64, outcome: Outcome },
}

impl CoreEvent {
    /// Events of one kind for one computer replace each other in the queue (newest wins);
    /// `None` for the ones that never coalesce.
    pub fn slot(&self) -> Option<(u8, &str)> {
        match self {
            CoreEvent::Computers { .. } => Some((0, "")),
            CoreEvent::Link { key, .. } => Some((1, key)),
            CoreEvent::Host { key, .. } => Some((2, key)),
            CoreEvent::Sessions { key, .. } => Some((3, key)),
            CoreEvent::Pair { .. } | CoreEvent::Answer { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn link_words_are_plain() {
        let w = |s: LinkState| words(&s, 10 * 3_600_000).0;
        assert_eq!(w(LinkState::Open { since_ms: 0 }), "Online");
        assert_eq!(w(LinkState::Dialing), "Connecting…");
        assert_eq!(w(LinkState::Securing), "Connecting…");
        assert_eq!(
            w(LinkState::Reconnecting {
                attempt: 5,
                in_ms: 3600
            }),
            "Reconnecting in 4 s"
        );
        assert_eq!(
            w(LinkState::Offline {
                seen_ms: Some(8 * 3_600_000)
            }),
            "Offline · seen 2 h ago"
        );
        assert_eq!(w(LinkState::Offline { seen_ms: None }), "Offline");
        assert_eq!(
            w(LinkState::Unreachable {
                attempt: 3,
                in_ms: 1000
            }),
            "Can't reach its relay"
        );
        assert_eq!(
            w(LinkState::Refused {
                why: Refusal::PinMismatch
            }),
            "Not this computer — re-pair"
        );
        assert_eq!(w(LinkState::RelayTooOld), "Relay too old for the app");
        assert_eq!(words(&LinkState::Open { since_ms: 0 }, 0).1, Tone::Live);
        assert_eq!(words(&LinkState::RelayTooOld, 0).1, Tone::Bad);
    }
}
