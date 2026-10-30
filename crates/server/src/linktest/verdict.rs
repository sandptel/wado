//! The speed test's verdict (the user's decision, `2026-10-30`): with what the device measured
//! and what this computer has to spare, would a session — or one more device in this one — work,
//! work worse, or not work at all?
//!
//! | level | means | the client |
//! |---|---|---|
//! | `ok` | nothing to say | offers it |
//! | `warn` | it works, worse — someone's picture drops, or the computer is busy | offers it, says why |
//! | `refuse` | it would not work: below the floor, out of memory, out of places | does not offer it |
//!
//! It also *fixes the number*: [`LinkVerdict::suggested_max`] is how many devices this computer's
//! upload carries at the floor, and the cap in force is the configured one lowered to it.
//!
//! Pure: every number comes in as an argument, so each rule is a test below.

use wado_protocol::multi::{FLOOR_KBPS, HostHeadroom, LinkVerdict};

/// What the verdict is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Intent {
    /// One more device in the session running here.
    Join,
    /// A new session.
    Start,
}

impl Intent {
    pub fn parse(s: &str) -> Self {
        if s == "start" {
            Self::Start
        } else {
            Self::Join
        }
    }
}

/// Everything the verdict needs.
#[derive(Debug, Clone)]
pub struct Ask {
    pub intent: Intent,
    /// What the device measured: download from this computer, round trip. `0` = not measured.
    pub down_kbps: u32,
    pub rtt_ms: u32,
    /// The shared stream's bitrate now — or, for a new session, what it would start at.
    pub session_kbps: u32,
    /// Devices in the session now (0 for a new one).
    pub participants: u32,
    /// `server { max-participants }`.
    pub cap: u32,
    pub host: HostHeadroom,
}

/// Only this much of a measured rate is counted on: the rest is the margin a stream needs.
const USABLE: f64 = 0.85;

pub fn judge(a: &Ask) -> LinkVerdict {
    let mut refuse: Vec<String> = Vec::new();
    let mut warn: Vec<String> = Vec::new();
    let mbps = |k: u32| format!("{:.1} Mbps", k as f64 / 1000.0);
    let usable = (a.down_kbps as f64 * USABLE) as u32;
    // The computer's upload is at least what any device has pulled from it.
    let uplink = a.host.uplink_kbps.max(a.down_kbps);
    let cap = a.cap.max(1);
    let suggested = if uplink == 0 {
        cap
    } else {
        ((uplink as f64 * USABLE) as u32 / FLOOR_KBPS).clamp(1, cap)
    };
    let max = cap.min(suggested);
    let after = a.participants + u32::from(a.intent == Intent::Join || a.participants == 0);

    // ── This device's own link ──
    if a.down_kbps > 0 {
        if usable < FLOOR_KBPS {
            refuse.push(format!(
                "this device's link carried {} — a desktop needs at least {}, so the picture \
                 would break up",
                mbps(a.down_kbps),
                mbps(FLOOR_KBPS)
            ));
        } else if a.intent == Intent::Join && usable < a.session_kbps {
            warn.push(format!(
                "this device's link is slower than the stream: everyone's picture drops to {} \
                 while it is in",
                mbps(usable)
            ));
        }
    }
    if a.rtt_ms > 600 {
        refuse.push(format!(
            "a round trip takes {} ms — every tap would land a second late",
            a.rtt_ms
        ));
    } else if a.rtt_ms > 150 {
        warn.push(format!(
            "a round trip takes {} ms — taps will feel slow",
            a.rtt_ms
        ));
    }

    // ── How many devices this computer's upload carries ──
    if a.intent == Intent::Join {
        if a.participants >= max {
            refuse.push(if max < cap {
                format!(
                    "this computer's upload (about {}) carries {max} device(s) at the lowest \
                     picture — {} are in already",
                    mbps(uplink),
                    a.participants
                )
            } else {
                format!("this session already has {max} devices — the most it takes")
            });
        } else if uplink > 0
            && (after as u64) * (a.session_kbps as u64) > (uplink as f64 * USABLE) as u64
        {
            let each = ((uplink as f64 * USABLE) as u32 / after.max(1)).max(FLOOR_KBPS);
            warn.push(format!(
                "this computer's upload is shared: with {after} devices everyone's picture drops \
                 to about {}",
                mbps(each)
            ));
        }
    }

    // ── The computer itself ──
    let h = &a.host;
    match a.intent {
        Intent::Start => {
            if h.max_sessions > 0 && h.sessions >= h.max_sessions {
                refuse.push(format!(
                    "this computer already runs {} sessions, the most it is set to — end or join \
                     one",
                    h.sessions
                ));
            }
            if h.mem_avail_mb > 0 && h.mem_avail_mb < 300 {
                refuse.push(format!(
                    "the computer has {} MB of memory free — a new desktop needs more",
                    h.mem_avail_mb
                ));
            } else if h.mem_avail_mb > 0 && h.mem_avail_mb < 800 {
                warn.push(format!(
                    "the computer is short of memory ({} MB free) — apps may be slow",
                    h.mem_avail_mb
                ));
            }
            if h.load_per_core > 1.5 {
                refuse.push(format!(
                    "the computer is flat out (load {:.1} per core) — a new desktop would stutter",
                    h.load_per_core
                ));
            } else if h.load_per_core > 0.9 {
                warn.push(format!(
                    "the computer is busy (load {:.1} per core)",
                    h.load_per_core
                ));
            }
            if h.encoder == "software" && h.sessions >= 1 {
                warn.push(
                    "another session already encodes on the CPU — a second will compete with it"
                        .into(),
                );
            }
        }
        Intent::Join => {
            if h.mem_avail_mb > 0 && h.mem_avail_mb < 64 {
                refuse.push(format!(
                    "the computer has {} MB of memory free",
                    h.mem_avail_mb
                ));
            }
            if h.load_per_core > 2.0 {
                warn.push(format!(
                    "the computer is flat out (load {:.1} per core)",
                    h.load_per_core
                ));
            }
        }
    }

    let level = if !refuse.is_empty() {
        "refuse"
    } else if !warn.is_empty() {
        "warn"
    } else {
        "ok"
    };
    let session_kbps = if a.intent == Intent::Join && usable >= FLOOR_KBPS {
        a.session_kbps.min(usable)
    } else {
        a.session_kbps
    };
    LinkVerdict {
        level: level.into(),
        reasons: refuse.into_iter().chain(warn).collect(),
        down_kbps: a.down_kbps,
        rtt_ms: a.rtt_ms,
        suggested_max: suggested,
        max_participants: max,
        participants: a.participants,
        session_kbps,
        host: HostHeadroom {
            uplink_kbps: uplink,
            ..a.host.clone()
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ask(intent: Intent, down: u32, participants: u32) -> Ask {
        Ask {
            intent,
            down_kbps: down,
            rtt_ms: 30,
            session_kbps: 8000,
            participants,
            cap: 4,
            host: HostHeadroom {
                mem_avail_mb: 8000,
                load_per_core: 0.2,
                encoder: "hardware".into(),
                sessions: 1,
                max_sessions: 4,
                uplink_kbps: 0,
            },
        }
    }

    #[test]
    fn a_fast_link_is_ok() {
        let v = judge(&ask(Intent::Join, 60_000, 1));
        assert_eq!(v.level, "ok", "{:?}", v.reasons);
        assert_eq!(v.max_participants, 4);
    }

    #[test]
    fn below_the_floor_is_refused() {
        let v = judge(&ask(Intent::Join, 1200, 1));
        assert!(v.refused());
        assert!(v.reasons[0].contains("1.2 Mbps"), "{:?}", v.reasons);
    }

    #[test]
    fn slower_than_the_stream_warns_and_names_the_new_rate() {
        let v = judge(&ask(Intent::Join, 5000, 1));
        assert_eq!(v.level, "warn");
        assert_eq!(v.session_kbps, 4250);
    }

    #[test]
    fn the_upload_fixes_how_many_devices() {
        // 5 Mbps up carries two devices at 1.5 Mbps (with the margin), not four.
        let mut a = ask(Intent::Join, 5000, 2);
        a.session_kbps = 2000;
        let v = judge(&a);
        assert_eq!((v.suggested_max, v.max_participants), (2, 2));
        assert!(
            v.refused(),
            "a third device would push everyone under the floor"
        );
        a.participants = 1;
        assert!(!judge(&a).refused());
    }

    #[test]
    fn the_configured_cap_holds_on_a_fast_link() {
        let v = judge(&ask(Intent::Join, 200_000, 4));
        assert!(v.refused());
        assert!(v.reasons[0].contains("4 devices"));
    }

    #[test]
    fn a_new_session_needs_memory_and_a_free_place() {
        let mut a = ask(Intent::Start, 20_000, 0);
        a.host.mem_avail_mb = 200;
        assert!(judge(&a).refused());
        a.host.mem_avail_mb = 600;
        assert_eq!(judge(&a).level, "warn");
        a.host.mem_avail_mb = 8000;
        a.host.sessions = 4;
        assert!(judge(&a).refused());
    }

    #[test]
    fn a_bad_round_trip_warns_then_refuses() {
        let mut a = ask(Intent::Join, 50_000, 1);
        a.rtt_ms = 200;
        assert_eq!(judge(&a).level, "warn");
        a.rtt_ms = 900;
        assert!(judge(&a).refused());
    }

    #[test]
    fn no_measurement_judges_the_host_alone() {
        let v = judge(&ask(Intent::Start, 0, 0));
        assert_eq!(v.level, "ok");
        assert_eq!(v.max_participants, 4);
    }
}
