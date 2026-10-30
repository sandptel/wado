//! The fallback lane's hints: what to say at the exact spot the relay's 64 KB/s cap is felt
//! (Decision Log 2026-10-27) — no picture, files on the lane or stalled, a big shell dump. The
//! detectors are `js/lane_hint.js`; the card is `ui/lane_hint.rs`. The rule for the words: it is
//! the network, not wado — name the cause when one end could tell, otherwise blame nobody — and
//! offer the one thing that helps.

use crate::link::Link;

/// Where the cap is felt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Spot {
    /// A session with no direct path: no picture at all.
    Picture,
    /// Files on the lane, a transfer it refused, or one that stopped moving.
    Files,
    /// Shell output arriving at the lane's pace.
    Shell,
}

impl Spot {
    pub fn from_js(s: &str) -> Option<Self> {
        match s {
            "files" => Some(Spot::Files),
            "shell" => Some(Spot::Shell),
            _ => None,
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Spot::Picture => "picture",
            Spot::Files => "files",
            Spot::Shell => "shell",
        }
    }
}

/// What the card says and offers.
#[derive(Clone, Debug, PartialEq)]
pub struct Hint {
    pub spot: Spot,
    pub title: String,
    pub body: String,
    /// Offer "Retry direct": only where a direct path could change it.
    pub retry: bool,
}

/// The words for `spot`, given the link as it is and the detector's `detail` (the computer's
/// own refusal, `stalled`, or nothing).
pub fn hint(spot: Spot, link: &Link, detail: &str) -> Hint {
    let direct = link.state == "direct";
    let cause = link.cause_words();
    match spot {
        Spot::Picture => Hint {
            spot,
            title: "No picture — there is no direct path".into(),
            body: format!(
                "Only the relay's limited lane is up (64 KB/s): shells, clipboard and files up to \
                 1 MB still work, the picture cannot. This is the network, not wado.{cause}"
            ),
            retry: true,
        },
        Spot::Files if detail == "stalled" => Hint {
            spot,
            title: "This transfer has stopped moving".into(),
            body: format!(
                "Nothing has arrived for a few seconds. That is the network, not wado — it \
                 carries on by itself when the path comes back.{}",
                if direct { "" } else { &cause }
            ),
            retry: !direct,
        },
        Spot::Files if !detail.is_empty() => Hint {
            spot,
            title: "That needs a direct connection".into(),
            body: detail.to_string(),
            retry: true,
        },
        Spot::Files => Hint {
            spot,
            title: "Files are on the relay's limited lane".into(),
            body: format!(
                "No direct path, so files move at about 64 KB/s, up to 1 MB each; thumbnails, \
                 folders as archives and playback wait for a direct connection. This is the \
                 network, not wado.{cause}"
            ),
            retry: true,
        },
        // Shells ride the relay link whatever the path, so on a direct link the cap is not the
        // network's doing and is not blamed on it.
        Spot::Shell if direct => Hint {
            spot,
            title: "Big output — arriving at the relay's pace".into(),
            body: "Shells always ride wado's relay, which carries 64 KB/s, so a dump this size \
                   takes a moment. Nothing is lost; it is still coming."
                .into(),
            retry: false,
        },
        Spot::Shell => Hint {
            spot,
            title: "Big output — on the relay's limited lane".into(),
            body: format!(
                "There is no direct path, so everything rides the relay at 64 KB/s and a dump \
                 this size takes a while. This is the network, not wado.{cause}"
            ),
            retry: true,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn link(state: &str) -> Link {
        Link {
            state: state.into(),
            ..Default::default()
        }
    }

    #[test]
    fn every_relayed_hint_blames_the_network_and_offers_retry() {
        for spot in [Spot::Picture, Spot::Files, Spot::Shell] {
            let h = hint(spot, &link("relayed"), "");
            assert!(h.body.contains("not wado"), "{spot:?}: {}", h.body);
            assert!(h.retry, "{spot:?}");
        }
    }

    #[test]
    fn a_shell_dump_on_a_direct_link_is_not_blamed_on_the_network() {
        let h = hint(Spot::Shell, &link("direct"), "");
        assert!(!h.body.contains("not wado") && !h.retry, "{}", h.body);
    }

    #[test]
    fn the_cause_is_named_only_when_known() {
        let warp = Link {
            cause: "warp".into(),
            ..link("relayed")
        };
        assert!(hint(Spot::Picture, &warp, "").body.contains("WARP"));
        assert!(!hint(Spot::Picture, &link("relayed"), "")
            .body
            .contains("WARP"));
        // The computer's own refusal is said as it is.
        let r = hint(Spot::Files, &link("relayed"), "3.0 MB is more than …");
        assert_eq!(r.body, "3.0 MB is more than …");
        assert!(hint(Spot::Files, &link("relayed"), "stalled")
            .title
            .contains("stopped"));
    }
}
