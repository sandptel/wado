//! The relay's **frozen handshake** — the only messages `wado-relay` ever parses.
//!
//! The relay is deployed once and has to outlive many daemon and client releases
//! (`WADO_PLAN.md`, Decision Log `2026-10-02`). It manages that by understanding as little as
//! possible: the handshake below, and nothing else. Every other frame is forwarded unread, so
//! a new session verb, a new field, a whole new feature between client and daemon needs no
//! relay change at all.
//!
//! Rules that keep this contract stable — break one and an old relay strands a new peer:
//! - **Fields are only ever added, each with `#[serde(default)]`.** Nothing is renamed or
//!   removed, and no type here uses `deny_unknown_fields`, so a newer peer's extra fields are
//!   ignored by an older relay and vice versa.
//! - **The relay forwards message types it does not know**; it never rejects them.
//! - **New relay behaviour is announced in `caps`**, so a peer uses it only when the relay it
//!   is talking to says it has it.
//!
//! [`crate::relay::RelayMsg`] carries the same variants for the daemon and the client, which
//! also need the session messages. The `same_wire_as_relay_msg` test pins the two to identical
//! JSON, so they cannot drift.

use serde::{Deserialize, Serialize};

/// The handshake version this build speaks.
///
/// `0` is what a peer from before versioning reports, by way of the serde default: its
/// handshake is exactly v1's minus the new optional fields, so `0` and `1` interoperate.
pub const WIRE_VERSION: u32 = 1;

/// WebSocket close code a client uses when it is done, not dropped: the relay frees its seat
/// instead of holding it. Announced as the `leave` cap; an older relay ignores the code.
pub const LEAVE_CLOSE_CODE: u16 = 4001;

/// WebSocket endpoint the **server** connects to in order to register itself.
/// Path: `ws://<relay>/register`
pub const RELAY_REGISTER_PATH: &str = "/register";

/// WebSocket endpoint the **client** connects to in order to join a server.
/// Path: `ws://<relay>/join/:remote_id`
pub const RELAY_JOIN_BASE_PATH: &str = "/join";

/// Canonicalize a Remote ID: strip the display separators (`-`, spaces) so
/// `528-491-307`, `528 491 307`, and `528491307` all compare equal. Both the
/// relay (register + join) and the server apply this before any comparison.
pub fn normalize_remote_id(id: &str) -> String {
    id.chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .collect()
}

/// Human-readable form of a Remote ID: 9 digits grouped as `XXX-XXX-XXX`.
/// Non-9-digit IDs (e.g. a custom `WADO_REMOTE_ID`) are returned unchanged.
pub fn display_remote_id(id: &str) -> String {
    if id.len() == 9 && id.chars().all(|c| c.is_ascii_digit()) {
        format!("{}-{}-{}", &id[0..3], &id[3..6], &id[6..9])
    } else {
        id.to_string()
    }
}

/// A handshake frame. See [`crate::relay::RelayMsg`] for what each variant means; the docs
/// live there because that is where the daemon and client read them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WireMsg {
    Register {
        remote_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        display_name: Option<String>,
        #[serde(default)]
        v: u32,
        #[serde(default)]
        instance_key: String,
        #[serde(default)]
        boot_id: String,
        #[serde(default)]
        hold_ms: u64,
        #[serde(default)]
        caps: Vec<String>,
    },
    Registered {
        remote_id: String,
        #[serde(default)]
        relay_v: u32,
        #[serde(default)]
        caps: Vec<String>,
    },
    PeerConnected {
        room_id: String,
        client_addr: String,
        #[serde(default)]
        client_key: String,
        #[serde(default)]
        client_name: String,
    },
    PeerCheck {
        room_id: String,
        client_addr: String,
        #[serde(default)]
        client_key: String,
        #[serde(default)]
        client_name: String,
    },
    PeerAccept {
        room_id: String,
    },
    PeerReject {
        room_id: String,
        #[serde(default)]
        reason: String,
    },
    PeerDisconnected {
        room_id: String,
    },
    SeatHold {
        room_id: String,
        #[serde(default)]
        hold_ms: u64,
    },
    Occupancy {
        #[serde(default)]
        session: bool,
    },
    JoinAccepted {
        remote_id: String,
        room_id: String,
        #[serde(default)]
        instance_id: String,
        #[serde(default)]
        pool_size: usize,
        #[serde(default)]
        pool_busy: usize,
        #[serde(default)]
        assignment: String,
        #[serde(default)]
        boot_id: String,
        #[serde(default)]
        relay_v: u32,
        #[serde(default)]
        caps: Vec<String>,
    },
    JoinDenied {
        reason: String,
        #[serde(default)]
        takeover: bool,
        #[serde(default)]
        retry_ms: u64,
    },
    Waiting {
        reason: String,
        #[serde(default)]
        ms_left: u64,
    },
    TakenOver {
        #[serde(default)]
        by: String,
    },
    Ping,
    Pong,
    Error {
        message: String,
    },
}

#[cfg(test)]
mod tests {
    use super::WireMsg;
    use crate::relay::RelayMsg;

    fn samples() -> Vec<WireMsg> {
        vec![
            WireMsg::Register {
                remote_id: "528491307".into(),
                display_name: Some("box".into()),
                v: 1,
                instance_key: "1".into(),
                boot_id: "b".into(),
                hold_ms: 1_800_000,
                caps: vec!["pong".into(), "gate".into()],
            },
            WireMsg::Registered {
                remote_id: "528491307".into(),
                relay_v: 1,
                caps: vec!["x".into()],
            },
            WireMsg::PeerConnected {
                room_id: "r".into(),
                client_addr: "1.2.3.4".into(),
                client_key: "k".into(),
                client_name: "Android · Chrome".into(),
            },
            WireMsg::PeerCheck {
                room_id: "r".into(),
                client_addr: "1.2.3.4".into(),
                client_key: "k".into(),
                client_name: "Laptop".into(),
            },
            WireMsg::PeerAccept {
                room_id: "r".into(),
            },
            WireMsg::PeerReject {
                room_id: "r".into(),
                reason: "denied".into(),
            },
            WireMsg::Waiting {
                reason: "no computer online".into(),
                ms_left: 5,
            },
            WireMsg::TakenOver { by: "Pixel".into() },
            WireMsg::PeerDisconnected {
                room_id: "r".into(),
            },
            WireMsg::SeatHold {
                room_id: "r".into(),
                hold_ms: 0,
            },
            WireMsg::Occupancy { session: true },
            WireMsg::JoinAccepted {
                remote_id: "528491307".into(),
                room_id: "r".into(),
                instance_id: "i".into(),
                pool_size: 2,
                pool_busy: 1,
                assignment: "assigned".into(),
                boot_id: "b".into(),
                relay_v: 1,
                caps: vec![],
            },
            WireMsg::JoinDenied {
                reason: "no".into(),
                takeover: true,
                retry_ms: 3,
            },
            WireMsg::Ping,
            WireMsg::Pong,
            WireMsg::Error {
                message: "e".into(),
            },
        ]
    }

    /// The relay speaks `WireMsg`; the daemon and client speak `RelayMsg`. Every handshake
    /// frame must mean the same thing to both, in both directions.
    #[test]
    fn same_wire_as_relay_msg() {
        for w in samples() {
            let json = serde_json::to_value(&w).unwrap();
            let r: RelayMsg = serde_json::from_value(json.clone())
                .unwrap_or_else(|e| panic!("RelayMsg cannot read {json}: {e}"));
            let back = serde_json::to_value(&r).unwrap();
            assert_eq!(json, back, "RelayMsg re-encodes {w:?} differently");
            assert_eq!(serde_json::from_value::<WireMsg>(back).unwrap(), w);
        }
    }

    /// Frames from peers that predate versioning — the exact JSON deployed today.
    #[test]
    fn pre_versioning_peers_still_parse() {
        let reg: WireMsg =
            serde_json::from_str(r#"{"type":"register","remote_id":"528491307"}"#).unwrap();
        assert!(matches!(reg, WireMsg::Register { v: 0, .. }));
        let ack: RelayMsg =
            serde_json::from_str(r#"{"type":"registered","remote_id":"528491307"}"#).unwrap();
        assert!(matches!(ack, RelayMsg::Registered { relay_v: 0, .. }));
        // An older relay's join_accepted, with none of the pool or version fields.
        let acc: RelayMsg =
            serde_json::from_str(r#"{"type":"join_accepted","remote_id":"1","room_id":"r"}"#)
                .unwrap();
        assert!(matches!(acc, RelayMsg::JoinAccepted { relay_v: 0, .. }));
    }

    /// A future peer's extra fields must not break an older reader.
    #[test]
    fn unknown_fields_are_ignored() {
        let reg: WireMsg = serde_json::from_str(
            r#"{"type":"register","remote_id":"1","v":9,"seat_hold_ms":5,"future":[1]}"#,
        )
        .unwrap();
        assert!(matches!(reg, WireMsg::Register { v: 9, .. }));
    }
}
