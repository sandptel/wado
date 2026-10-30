//! The core's events and views as UniFFI records — a plain mirror, so `wado-client-core` stays
//! free of FFI. Kotlin never parses JSON for any of these.

use wado_client_core::event as ev;
use wado_client_core::feeds::{host, sessions};
use wado_client_core::link::state::LinkState;
use wado_client_core::pair::flow::PairStep;
use wado_client_core::request::Outcome;

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct ComputerInfo {
    pub key: String,
    pub name: String,
    pub remote_id: String,
    pub relay_url: String,
    pub os: String,
    pub logo: String,
    pub desktop: String,
    pub hue: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, uniffi::Enum)]
pub enum LinkTone {
    Live,
    Busy,
    Still,
    Bad,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct LinkInfo {
    /// `idle`, `dialing`, `securing`, `open`, `reconnecting`, `unreachable`, `offline`,
    /// `refused`, `relay_too_old`.
    pub kind: String,
    pub words: String,
    pub tone: LinkTone,
    /// The state's technical name, for Diagnostics.
    pub tech: String,
    pub caps: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct NowPlayingInfo {
    pub app: String,
    pub title: String,
    pub artist: String,
    pub playing: bool,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct HostInfo {
    pub name: String,
    pub os: String,
    pub logo: String,
    pub desktop: String,
    pub uptime_s: u64,
    pub caps: Vec<String>,
    pub playing: Option<NowPlayingInfo>,
    pub volume: Option<f32>,
    pub brightness: Option<f32>,
    pub awake: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct SessionInfo {
    pub instance: String,
    pub apps_first: String,
    pub apps_more: u32,
    pub lead: String,
    pub participants: u32,
}

#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum PairProgress {
    Dialing,
    Securing,
    CheckWords { words: Vec<String> },
    WaitingForComputer,
    Done { key: String },
    Failed { why: String },
}

#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum OpOutcome {
    Done,
    Failed { why: String },
    NoAnswer,
}

#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum CoreEvent {
    Computers { list: Vec<ComputerInfo> },
    Link { key: String, link: LinkInfo },
    Host { key: String, host: HostInfo },
    Sessions { key: String, rows: Vec<SessionInfo> },
    Pair { op: u64, step: PairProgress },
    Answer { op: u64, outcome: OpOutcome },
}

pub fn computer(c: ev::ComputerView) -> ComputerInfo {
    ComputerInfo {
        key: c.key,
        name: c.name,
        remote_id: c.remote_id,
        relay_url: c.relay_url,
        os: c.os,
        logo: c.logo,
        desktop: c.desktop,
        hue: c.hue,
    }
}

fn kind(s: &LinkState) -> &'static str {
    match s {
        LinkState::Idle => "idle",
        LinkState::Dialing => "dialing",
        LinkState::Securing => "securing",
        LinkState::Open { .. } => "open",
        LinkState::Reconnecting { .. } => "reconnecting",
        LinkState::Unreachable { .. } => "unreachable",
        LinkState::Offline { .. } => "offline",
        LinkState::Refused { .. } => "refused",
        LinkState::RelayTooOld => "relay_too_old",
    }
}

fn link(v: ev::LinkView) -> LinkInfo {
    LinkInfo {
        kind: kind(&v.state).into(),
        words: v.words,
        tone: match v.tone {
            ev::Tone::Live => LinkTone::Live,
            ev::Tone::Busy => LinkTone::Busy,
            ev::Tone::Still => LinkTone::Still,
            ev::Tone::Bad => LinkTone::Bad,
        },
        tech: v.tech,
        caps: v.caps,
    }
}

fn host(v: host::HostView) -> HostInfo {
    HostInfo {
        name: v.name,
        os: v.os,
        logo: v.logo.to_string(),
        desktop: v.desktop,
        uptime_s: v.uptime_s,
        caps: v.caps,
        playing: v.playing.map(|p| NowPlayingInfo {
            app: p.app,
            title: p.title,
            artist: p.artist,
            playing: p.playing,
        }),
        volume: v.volume,
        brightness: v.brightness,
        awake: v.awake,
    }
}

fn session(r: sessions::SessionRow) -> SessionInfo {
    SessionInfo {
        instance: r.instance,
        apps_first: r.apps_first,
        apps_more: r.apps_more,
        lead: r.lead,
        participants: r.participants,
    }
}

fn step(s: PairStep) -> PairProgress {
    match s {
        PairStep::Dialing => PairProgress::Dialing,
        PairStep::Securing => PairProgress::Securing,
        PairStep::CheckWords { words } => PairProgress::CheckWords {
            words: words.to_vec(),
        },
        PairStep::WaitingForComputer => PairProgress::WaitingForComputer,
        PairStep::Done { key } => PairProgress::Done { key },
        PairStep::Failed { why } => PairProgress::Failed { why },
    }
}

fn outcome(o: Outcome) -> OpOutcome {
    match o {
        Outcome::Done => OpOutcome::Done,
        Outcome::Failed(why) => OpOutcome::Failed { why },
        Outcome::NoAnswer => OpOutcome::NoAnswer,
    }
}

pub fn event(e: ev::CoreEvent) -> CoreEvent {
    match e {
        ev::CoreEvent::Computers { list } => CoreEvent::Computers {
            list: list.into_iter().map(computer).collect(),
        },
        ev::CoreEvent::Link { key, view } => CoreEvent::Link {
            key,
            link: link(view),
        },
        ev::CoreEvent::Host { key, view } => CoreEvent::Host {
            key,
            host: host(view),
        },
        ev::CoreEvent::Sessions { key, rows } => CoreEvent::Sessions {
            key,
            rows: rows.into_iter().map(session).collect(),
        },
        ev::CoreEvent::Pair { op, step: s } => CoreEvent::Pair { op, step: step(s) },
        ev::CoreEvent::Answer { op, outcome: o } => CoreEvent::Answer {
            op,
            outcome: outcome(o),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn link_events_carry_plain_words_and_kind() {
        let e = event(ev::CoreEvent::Link {
            key: "1".into(),
            view: ev::LinkView::new(LinkState::RelayTooOld, 0, vec!["leave".into()]),
        });
        let CoreEvent::Link { key, link } = e else {
            panic!()
        };
        assert_eq!(key, "1");
        assert_eq!(link.kind, "relay_too_old");
        assert_eq!(link.words, "Relay too old for the app");
        assert_eq!(link.tone, LinkTone::Bad);
        assert_eq!(link.caps, ["leave"]);
    }
}
