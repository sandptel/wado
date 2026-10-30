//! Trackpad mode's wire — the phone acts as the host computer's multi-touch touchpad and
//! keyboard, with no picture and no session. Two data channels carry [`PadMsg`]: touches ride
//! an unordered, unreliable channel because a position is latest-wins (the same reasoning as
//! [`crate::MOTION_CHANNEL`]); keys ride an ordered, reliable one because a dropped keystroke
//! is unrecoverable. Relayed, the same messages ride [`crate::relay::RelayMsg::PadIn`] /
//! [`crate::relay::RelayMsg::PadOut`].

use serde::{Deserialize, Serialize};

/// Touch contacts: unordered, `maxRetransmits` 0. A late or lost frame is superseded by the
/// next one, so retransmitting it is wasted latency, not a correctness need.
pub const TOUCH_CHANNEL: &str = "pad-touch";

/// Keys: ordered, reliable. A dropped keystroke is unrecoverable, unlike a touch position.
pub const KEYS_CHANNEL: &str = "pad-keys";

/// One finger: `(id, x, y)`, `x`/`y` normalised 0..=65535 over the pad.
pub type Contact = (u8, u16, u16);

/// A keyboard modifier, as the client names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mod {
    Ctrl,
    Alt,
    Shift,
    Super,
}

/// One message on either trackpad-mode channel.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "k", rename_all = "snake_case")] // not "t": Frame has a field `t`
pub enum PadMsg {
    /// The pad's size in CSS px. Sent first, and again on every resize.
    Size { w: u16, h: u16 },
    /// Every finger down right now. Empty = all lifted. `t` = client ms, wrapping.
    Frame { seq: u32, t: u32, c: Vec<Contact> },
    /// A named key (`esc`, `tab`, `enter`, `backspace`, `delete`, `left`, `right`, `up`,
    /// `down`, `home`, `end`, `pageup`, `pagedown`, `space`, `f1`…`f12`) tapped with `mods`.
    Key { key: String, mods: Vec<Mod> },
    /// One character tapped as a key with `mods` (a latched modifier + the next character).
    Char { ch: char, mods: Vec<Mod> },
    /// Modifiers held down until the next `Hold` (a locked modifier). `[]` releases all.
    Hold { mods: Vec<Mod> },
    /// Committed text: typed as text, any Unicode.
    Text { text: String },
    /// `bin`: this phone can send compact touch frames ([`crate::pad_bin`]) once told it may.
    Ping {
        t: u32,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        bin: bool,
    },
    /// Daemon → phone only. `bin`: send touch frames compact from now on — only ever in answer
    /// to a ping that offered them, so an older phone is never told about them.
    Pong {
        t: u32,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        bin: bool,
    },
    /// Phone → daemon, relayed: a [`PadMsg::Frame`] in [`crate::pad_bin`]'s bytes, base64
    /// (standard alphabet, padded — what the browser's `btoa` writes). Sent only to a daemon
    /// whose pong said `bin`.
    Bin { b: String },
    /// Daemon → phone only: a message was refused, or a device could not be made. `fatal`: the
    /// daemon closed the pad, so the phone should stop sending.
    Err {
        message: String,
        #[serde(default)]
        fatal: bool,
    },
    /// Phone → daemon: the trackpad was closed. Let go of everything and remove the devices.
    Bye,
    /// Phone → daemon: what can run here? Needs the `host` grant, not `input`.
    Actions,
    /// Daemon → phone: the actions this computer can run now — only those whose tools exist.
    ActionList { actions: Vec<ActionInfo> },
    /// Phone → daemon: run the action `id`. Needs `host`; answered by [`PadMsg::Ran`].
    Run { id: String },
    /// Daemon → phone: how a [`PadMsg::Run`] went. `message` says why, in plain words, when not.
    Ran {
        id: String,
        ok: bool,
        #[serde(default)]
        message: String,
    },
    /// Phone → daemon, keys channel: the Gamepad screen opened — make the controller (or keep
    /// the one still there from a moment ago). Answered by [`PadMsg::Game`].
    GameOn,
    /// Phone → daemon, keys channel: the Gamepad screen closed. Everything is let go now; the
    /// controller itself stays a while, so a game does not see it unplugged and plugged back.
    GameOff,
    /// Phone → daemon, keys channel: one gamepad button, by evdev code — `BTN_A` 0x130 …
    /// `BTN_THUMBR` 0x13e, the digital triggers `BTN_TL2` / `BTN_TR2` (0x138 / 0x139, written
    /// as `ABS_Z` / `ABS_RZ`), and the d-pad's `BTN_DPAD_UP` … `RIGHT` (0x220–0x223, written as
    /// the hat). An unknown code is ignored.
    Btn { code: u16, down: bool },
    /// Phone → daemon, touch channel: every axis at once, newest wins — `[lx, ly, rx, ry, lt,
    /// rt]`, sticks −32768..=32767, triggers 0..=255. Re-sent every 100 ms while any is off
    /// centre; silence centres them.
    Axes { seq: u32, a: [i32; 6] },
    /// Daemon → phone: the controller exists. `player` is its order among wado's controllers on
    /// this computer (a session's own pad and this one), counted from 1 — what games number by.
    Game { player: u8 },
}

/// The gamepad buttons [`PadMsg::Btn`] carries that are not plain `BTN_*` buttons of the device.
pub mod btn {
    /// Digital triggers, written as the trigger axes at full travel.
    pub const TL2: u16 = 0x138;
    pub const TR2: u16 = 0x139;
    /// The d-pad's four directions, written as the hat.
    pub const DPAD_UP: u16 = 0x220;
    pub const DPAD_DOWN: u16 = 0x221;
    pub const DPAD_LEFT: u16 = 0x222;
    pub const DPAD_RIGHT: u16 = 0x223;
}

/// One action the phone may offer: `workspace-next`, "Next workspace", group `Workspace`, and
/// an icon name from the client's icon set.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionInfo {
    pub id: String,
    pub name: String,
    /// `Window`, `Workspace`, `Media`, `Sound`, `System` or `Apps`.
    pub group: String,
    pub icon: String,
}

/// Is `seq` newer than `last`, across `u32` wrap.
pub fn newer(seq: u32, last: u32) -> bool {
    seq.wrapping_sub(last).wrapping_sub(1) < u32::MAX / 2
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_round_trips_compactly() {
        let m = PadMsg::Frame {
            seq: 7,
            t: 1000,
            c: vec![(0, 100, 65535)],
        };
        let s = serde_json::to_string(&m).unwrap();
        assert_eq!(s, r#"{"k":"frame","seq":7,"t":1000,"c":[[0,100,65535]]}"#);
        assert_eq!(serde_json::from_str::<PadMsg>(&s).unwrap(), m);
    }

    #[test]
    fn actions_round_trip_tagged_k() {
        let cases = [
            (PadMsg::Actions, r#"{"k":"actions"}"#),
            (
                PadMsg::ActionList {
                    actions: vec![ActionInfo {
                        id: "workspace-next".into(),
                        name: "Next workspace".into(),
                        group: "Workspace".into(),
                        icon: "right".into(),
                    }],
                },
                r#"{"k":"action_list","actions":[{"id":"workspace-next","name":"Next workspace","group":"Workspace","icon":"right"}]}"#,
            ),
            (
                PadMsg::Run {
                    id: "fullscreen".into(),
                },
                r#"{"k":"run","id":"fullscreen"}"#,
            ),
            (
                PadMsg::Ran {
                    id: "lock".into(),
                    ok: false,
                    message: "no".into(),
                },
                r#"{"k":"ran","id":"lock","ok":false,"message":"no"}"#,
            ),
        ];
        for (m, s) in cases {
            assert_eq!(serde_json::to_string(&m).unwrap(), s);
            assert_eq!(serde_json::from_str::<PadMsg>(s).unwrap(), m);
        }
        // `message` may be left out.
        assert_eq!(
            serde_json::from_str::<PadMsg>(r#"{"k":"ran","id":"x","ok":true}"#).unwrap(),
            PadMsg::Ran {
                id: "x".into(),
                ok: true,
                message: String::new()
            }
        );
    }

    #[test]
    fn compact_frames_are_offered_and_accepted_without_breaking_older_peers() {
        // An older phone's ping, and an older daemon's pong, carry no `bin`.
        assert_eq!(
            serde_json::from_str::<PadMsg>(r#"{"k":"ping","t":5}"#).unwrap(),
            PadMsg::Ping { t: 5, bin: false }
        );
        assert_eq!(
            serde_json::to_string(&PadMsg::Pong { t: 5, bin: false }).unwrap(),
            r#"{"k":"pong","t":5}"#
        );
        // A newer one's say so, and an older reader ignores the field.
        assert_eq!(
            serde_json::to_string(&PadMsg::Ping { t: 5, bin: true }).unwrap(),
            r#"{"k":"ping","t":5,"bin":true}"#
        );
        assert_eq!(
            serde_json::from_str::<PadMsg>(r#"{"k":"bin","b":"AAAABwAAA+gAAGT//w=="}"#).unwrap(),
            PadMsg::Bin {
                b: "AAAABwAAA+gAAGT//w==".into()
            }
        );
    }

    #[test]
    fn newer_survives_wrap() {
        assert!(newer(1, 0));
        assert!(!newer(0, 0));
        assert!(newer(2, u32::MAX - 1));
        assert!(!newer(u32::MAX - 1, 2));
    }

    #[test]
    fn gamepad_messages_round_trip_tagged_k() {
        let cases = [
            (PadMsg::GameOn, r#"{"k":"game_on"}"#),
            (PadMsg::GameOff, r#"{"k":"game_off"}"#),
            (
                PadMsg::Btn {
                    code: 304,
                    down: true,
                },
                r#"{"k":"btn","code":304,"down":true}"#,
            ),
            (
                PadMsg::Axes {
                    seq: 3,
                    a: [-32768, 32767, 0, 5, 0, 255],
                },
                r#"{"k":"axes","seq":3,"a":[-32768,32767,0,5,0,255]}"#,
            ),
            (PadMsg::Game { player: 2 }, r#"{"k":"game","player":2}"#),
        ];
        for (m, s) in cases {
            assert_eq!(serde_json::to_string(&m).unwrap(), s);
            assert_eq!(serde_json::from_str::<PadMsg>(s).unwrap(), m);
        }
        assert!(
            serde_json::from_str::<PadMsg>(r#"{"k":"axes","seq":1,"a":[0,0,0]}"#).is_err(),
            "the axes are always all six"
        );
    }

    #[test]
    fn gamepad_messages_ride_the_relay_unread() {
        for msg in [
            PadMsg::GameOn,
            PadMsg::Btn {
                code: btn::DPAD_UP,
                down: false,
            },
            PadMsg::Axes {
                seq: 9,
                a: [1, 2, 3, 4, 5, 6],
            },
        ] {
            let r = crate::relay::RelayMsg::PadIn { msg: msg.clone() };
            let s = serde_json::to_string(&r).unwrap();
            assert!(s.starts_with(r#"{"type":"pad_in""#), "{s}");
            match serde_json::from_str::<crate::relay::RelayMsg>(&s).unwrap() {
                crate::relay::RelayMsg::PadIn { msg: back } => assert_eq!(back, msg),
                other => panic!("{other:?}"),
            }
        }
        let out = crate::relay::RelayMsg::PadOut {
            msg: PadMsg::Game { player: 1 },
        };
        assert!(serde_json::to_string(&out)
            .unwrap()
            .starts_with(r#"{"type":"pad_out""#));
    }

    #[test]
    fn pad_in_rides_the_relay_enum() {
        let r = crate::relay::RelayMsg::PadIn {
            msg: PadMsg::Hold {
                mods: vec![Mod::Super],
            },
        };
        let s = serde_json::to_string(&r).unwrap();
        assert!(s.starts_with(r#"{"type":"pad_in""#));
    }
}
