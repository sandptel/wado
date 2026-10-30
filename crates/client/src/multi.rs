//! Several devices in one session, as this page knows it (Decision Log `2026-10-30`): what the
//! relay can carry, who is in the session it is watching, what it is in the middle of, and the
//! last speed test. Filled by `js/multi.js`, `js/linktest.js` and `js/relay_link.js` through
//! [`crate::bridge`]; read by the Computer page's Sessions section.
//!
//! What a session row may offer is decided here, as plain functions, so every rule is a test.

use wado_protocol::{LinkVerdict, Participant, SessionSummary};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Multi {
    /// The relay's caps, from its last `join_accepted`.
    pub caps: Vec<String>,
    /// Who is in the session this page is in; `you` is this page's id among them.
    pub list: Vec<Participant>,
    pub you: u16,
    /// What is under way: `join`, `takeover`, `end` (on `instance`) or `start`; empty when idle.
    pub busy: (String, String),
    /// The speed test: running, and the last answer.
    pub testing: bool,
    pub verdict: Option<LinkVerdict>,
}

impl Multi {
    /// The relay can carry joined rooms: seatless, strictly targeted.
    pub fn rooms(&self) -> bool {
        self.caps.iter().any(|c| c == "control") && self.caps.iter().any(|c| c == "room_strict")
    }
}

/// What a session row offers this device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Offer {
    /// Open the session this page is in, or resume its own.
    pub rejoin: bool,
    /// Join it as one more participant.
    pub join: bool,
    pub takeover: bool,
    pub end: bool,
    /// Why Join is not offered, when it is not — said under the row rather than hidden.
    pub why_not: &'static str,
}

/// `here`: the instance this page's link is on. `granted`: this device's (join, takeover), when
/// the daemon said (`None`: an older daemon — offer and let it say no).
pub fn offer(
    s: &SessionSummary,
    here: &str,
    in_it: bool,
    rooms: bool,
    granted: Option<(bool, bool)>,
) -> Offer {
    let (may_join, may_take) = granted.unwrap_or((true, true));
    let full = s.max_participants > 0 && s.participants.len() as u32 >= s.max_participants;
    let mine = s.instance == here;
    let nobody = s.participants.is_empty() && s.viewer.is_none();
    let joinable = s.max_participants > 0;
    let mut o = Offer {
        end: true,
        ..Default::default()
    };
    if in_it {
        o.rejoin = true;
        return o;
    }
    // Nobody in it (left running, or this device's own): resuming is today's Rejoin.
    if nobody || (mine && !joinable) {
        o.rejoin = true;
        return o;
    }
    o.join = rooms && joinable && may_join && !full;
    o.takeover = may_take;
    o.why_not = if !joinable {
        "its computer's wado is older than joining — take it over instead"
    } else if !rooms {
        "the relay is older than joining — take it over instead"
    } else if !may_join {
        "this device may not join others' sessions — on the computer: wado allow <device> join"
    } else if full {
        "it is full — the most devices it carries are in"
    } else {
        ""
    };
    o
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(n_participants: usize, max: u32) -> SessionSummary {
        SessionSummary {
            instance: "1:2".into(),
            participants: (0..n_participants)
                .map(|i| Participant {
                    id: i as u16 + 1,
                    name: format!("D{i}"),
                    lead: i == 0,
                    ..Default::default()
                })
                .collect(),
            max_participants: max,
            viewer: (n_participants > 0).then(|| "D0".into()),
            ..Default::default()
        }
    }

    #[test]
    fn someone_elses_session_offers_join_and_take_over() {
        let o = offer(&s(1, 4), "1:1", false, true, Some((true, true)));
        assert!(o.join && o.takeover && o.end && !o.rejoin);
        assert_eq!(o.why_not, "");
    }

    #[test]
    fn a_full_one_offers_take_over_and_says_why_not_join() {
        let o = offer(&s(4, 4), "1:1", false, true, None);
        assert!(!o.join && o.takeover);
        assert!(o.why_not.contains("full"));
    }

    #[test]
    fn an_old_relay_or_daemon_never_offers_join() {
        let o = offer(&s(1, 4), "1:1", false, false, None);
        assert!(!o.join && o.takeover && o.why_not.contains("relay"));
        let o = offer(&s(1, 0), "1:1", false, true, None);
        assert!(!o.join && o.why_not.contains("older"));
    }

    #[test]
    fn nobody_in_it_is_a_plain_rejoin() {
        let o = offer(&s(0, 4), "1:1", false, true, None);
        assert!(o.rejoin && !o.join && !o.takeover);
    }

    #[test]
    fn a_missing_grant_is_said() {
        let o = offer(&s(1, 4), "1:1", false, true, Some((false, true)));
        assert!(!o.join && o.why_not.contains("wado allow"));
    }

    #[test]
    fn rooms_need_both_caps() {
        let mut m = Multi::default();
        m.caps = vec!["control".into()];
        assert!(!m.rooms());
        m.caps.push("room_strict".into());
        assert!(m.rooms());
    }
}
