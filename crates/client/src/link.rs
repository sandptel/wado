//! The link badge's model: which path this device is on (from `js/link_state.js`), and how to
//! say it. The wording lives here so the badge and the overlay agree — and so the limited relay
//! lane is always named as the network's doing, never as the product (Decision Log 2026-10-27).

use serde::Deserialize;

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct Link {
    /// `offline` | `computer_offline` | `waiting` | `ready` | `connecting` | `relayed` |
    /// `reconnecting` | `direct`.
    pub state: String,
    /// With `direct`: `lan` | `internet` | `turn`.
    pub path: String,
    pub rtt_ms: Option<u32>,
    /// This device's network, as the browser names it (`wifi`, `cellular`, `4g`, …).
    pub net: String,
    /// The computer's NAT, from its startup probe: `consistent` | `symmetric` | `unknown`.
    pub nat: String,
    pub turn: bool,
    /// Why `offline` (`no_network` | `relay_unreachable`) or why `waiting` (the relay's words).
    pub why: String,
}

/// How the badge is coloured.
#[derive(Clone, Copy, PartialEq)]
pub enum Tone {
    Good,
    Warn,
    Bad,
}

impl Tone {
    pub fn class(self) -> &'static str {
        match self {
            Tone::Good => "good",
            Tone::Warn => "warn",
            Tone::Bad => "bad",
        }
    }
}

impl Link {
    /// The badge's word or two.
    pub fn label(&self) -> String {
        match self.state.as_str() {
            "direct" => match self.path.as_str() {
                "lan" => "Direct · LAN".into(),
                "turn" => "Via TURN".into(),
                _ => "Direct · internet".into(),
            },
            "ready" => "Connected".into(),
            "connecting" => "Connecting…".into(),
            "relayed" => "Relayed — limited".into(),
            "reconnecting" => "Reconnecting…".into(),
            "computer_offline" => "Computer offline".into(),
            "waiting" => "Waiting".into(),
            _ if self.why == "no_network" => "No network".into(),
            _ => "Offline".into(),
        }
    }

    pub fn tone(&self) -> Tone {
        match self.state.as_str() {
            "direct" | "ready" => Tone::Good,
            "relayed" | "reconnecting" | "waiting" | "connecting" => Tone::Warn,
            _ => Tone::Bad,
        }
    }

    /// The sentence under the badge when it is opened: what this means, and whose it is.
    pub fn detail(&self) -> String {
        let net = if self.net.is_empty() {
            String::new()
        } else {
            format!(" This device is on {}.", self.net)
        };
        match self.state.as_str() {
            "direct" => {
                let rtt = self
                    .rtt_ms
                    .map_or(String::new(), |ms| format!(" {ms} ms round trip."));
                let how = match self.path.as_str() {
                    "lan" => "Straight across your local network.",
                    "turn" => "Through a TURN server — direct was not possible.",
                    _ => "Straight across the internet — the relay only introduced you.",
                };
                format!("{how}{rtt}{net}")
            }
            "ready" => format!(
                "Joined through the relay. The picture connects directly when you start.{net}"
            ),
            "relayed" => format!(
                "No direct path, so only the relay link is up. It carries small things — shells, \
                 clipboard, status — but not the picture or files. This is the network, not \
                 wado: switching off a VPN or moving to mobile data usually fixes it.{}{net}",
                self.nat_hint()
            ),
            "connecting" => format!("Setting up the direct path to your computer.{net}"),
            "reconnecting" => {
                format!(
                    "The network changed — getting the picture back. Your session is safe.{net}"
                )
            }
            "computer_offline" => "The relay is up, but your computer is not connected to it. \
                 Is `wado daemon` running there?"
                .into(),
            "waiting" => format!("The relay is holding this connection: {}.", self.why),
            _ if self.why == "no_network" => "This device has no network connection.".into(),
            _ => "wado's relay is unreachable — this is not your computer. Retrying.".into(),
        }
    }

    /// A likely cause on the computer's side, from its NAT probe — said only when it applies.
    fn nat_hint(&self) -> String {
        if self.nat == "symmetric" && !self.turn {
            " Your computer is behind a VPN or a strict NAT, which blocks direct connections — \
             switching its VPN off is the likely fix."
                .into()
        } else {
            String::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn link(state: &str, path: &str) -> Link {
        Link {
            state: state.into(),
            path: path.into(),
            ..Default::default()
        }
    }

    #[test]
    fn the_limited_lane_is_never_called_connected_and_blames_the_network() {
        let r = link("relayed", "");
        assert_eq!(r.label(), "Relayed — limited");
        assert!(r.tone() == Tone::Warn);
        assert!(r.detail().contains("not wado"));
        assert!(
            !r.detail().contains("VPN or a strict NAT"),
            "no NAT hint without a verdict"
        );
        let r = Link {
            nat: "symmetric".into(),
            ..r
        };
        assert!(r.detail().contains("VPN or a strict NAT"));
        assert_eq!(link("direct", "lan").label(), "Direct · LAN");
        let off = Link {
            why: "relay_unreachable".into(),
            ..link("offline", "")
        };
        assert!(off.detail().contains("not your computer"));
    }
}
