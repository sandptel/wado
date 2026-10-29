//! Multi-client sessions (Decision Log `2026-10-30`, spec
//! `docs/superpowers/specs/2026-10-30-multi-client-design.md`): who is in a session, the link
//! speed test that guards how many may be, and the presence dots.
//!
//! All of it rides the end-to-end envelope, daemon ↔ client; the relay never reads it. Every field
//! defaults, so an older daemon, client or `sessions/` file still parses.

use serde::{Deserialize, Serialize};

/// Devices per session when nothing else says (`server { max-participants }`).
pub const DEFAULT_MAX_PARTICIPANTS: u32 = 4;

/// No participant's bitrate cap drags the shared stream below this (kbps). Below it a viewer is
/// labelled "slowed by …" and the floor holds.
pub const FLOOR_KBPS: u32 = 1500;

/// The data channel the presence dots ride: unordered, no retransmits — never the input channels
/// (invariant 1), never the relay.
pub const PRESENCE_CHANNEL: &str = "wado-presence";

/// One device in a session, as the Computer page lists it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Participant {
    /// This participant's number in the session — the daemon's, never a device key. A
    /// `participants` message names the receiver's own (`you`), and presence dots use it.
    pub id: u16,
    /// The device's name ("Pixel 8 · Chrome").
    pub name: String,
    /// It started the session or last took it over: the output is its size.
    pub lead: bool,
    /// Joined at, unix ms.
    pub since_ms: u64,
    /// How it reaches the computer (`direct · lan · 4 ms`), when it has said.
    pub link: String,
    /// The device's colour on the others' screens, 0..8 — stable per device, so a dot and a
    /// chip agree.
    pub hue: u8,
}

/// The speed test's answer: how a session would be for this device, and how many devices this
/// computer's link can carry. Computed by the daemon (`server::linktest`), shown by the client.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LinkVerdict {
    /// `ok`, `warn` (it works, worse — say so) or `refuse` (it would not work: not offered).
    pub level: String,
    /// Plain sentences, worst first.
    pub reasons: Vec<String>,
    /// What the test measured, client side: download from the computer, and the round trip.
    pub down_kbps: u32,
    pub rtt_ms: u32,
    /// How many devices one session on this computer can carry, from what was measured.
    pub suggested_max: u32,
    /// The limit in force now: `server { max-participants }`, lowered to `suggested_max`.
    pub max_participants: u32,
    /// How many are in the session it was asked about (0 for a new one).
    pub participants: u32,
    /// The stream's bitrate for everyone, kbps, if this device were in.
    pub session_kbps: u32,
    /// The computer's own headroom.
    pub host: HostHeadroom,
}

impl LinkVerdict {
    pub fn refused(&self) -> bool {
        self.level == "refuse"
    }
}

/// The computer's own headroom, as the speed test reads it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HostHeadroom {
    /// `MemAvailable`, MB.
    pub mem_avail_mb: u32,
    /// The 1-minute load average per CPU core.
    pub load_per_core: f32,
    /// The encoder sessions are using: `hardware`, `software` or empty (none running).
    pub encoder: String,
    /// Sessions running on this computer, and the most it will run.
    pub sessions: u32,
    pub max_sessions: u32,
    /// The best download any device measured from here lately — a floor on this computer's
    /// upload — kbps.
    pub uplink_kbps: u32,
}

/// A ghost cursor: where another participant last pointed, normalized to the output.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Presence {
    /// Stable per participant for this session (never a device key).
    pub id: u16,
    pub name: String,
    pub hue: u8,
    pub x: f64,
    pub y: f64,
    /// `pointer` or `touch`; empty once it has left.
    pub kind: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_object_is_every_type() {
        assert_eq!(
            serde_json::from_str::<Participant>("{}").unwrap(),
            Participant::default()
        );
        assert!(!serde_json::from_str::<LinkVerdict>("{}").unwrap().refused());
        assert_eq!(
            serde_json::from_str::<Presence>(r#"{"id":3,"x":0.5,"y":0.25}"#)
                .unwrap()
                .id,
            3
        );
    }

    #[test]
    fn a_verdict_round_trips() {
        let v = LinkVerdict {
            level: "refuse".into(),
            reasons: vec!["too slow".into()],
            down_kbps: 900,
            suggested_max: 2,
            max_participants: 2,
            host: HostHeadroom {
                mem_avail_mb: 4000,
                encoder: "hardware".into(),
                ..Default::default()
            },
            ..Default::default()
        };
        let back: LinkVerdict = serde_json::from_str(&serde_json::to_string(&v).unwrap()).unwrap();
        assert_eq!(back, v);
        assert!(back.refused());
    }
}
