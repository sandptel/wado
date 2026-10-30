//! Core events as the JSON objects the Dart side reads (`app/lib/core/core_host.dart`). Each has
//! `t`, its kind:
//!
//! - `computers` `{list: [{key, name, remote_id, relay_url, os, logo, desktop, hue}]}`
//! - `room_state` `{id, state, words, tone, caps, instance, in_ms}`
//! - `room_msg` / `room_relay` `{id, json}`: the message or relay frame as received
//! - `pair` `{op, step, words?, key?, why?}`, step one of dialing securing check_words waiting
//!   done failed
//! - `answer` `{op, outcome, why?}`, outcome one of done failed no_answer
//! - `other` for the feeds the app does not read (it opens its own rooms).

use serde_json::{json, Value};
use wado_client_core::event::{ComputerView, CoreEvent, LinkView, Tone};
use wado_client_core::link::state::LinkState;
use wado_client_core::pair::flow::PairStep;
use wado_client_core::request::Outcome;

fn computer(c: &ComputerView) -> Value {
    json!({"key": c.key, "name": c.name, "remote_id": c.remote_id, "relay_url": c.relay_url,
           "os": c.os, "logo": c.logo, "desktop": c.desktop, "hue": c.hue})
}

pub(crate) fn computers_json(list: &[ComputerView]) -> String {
    Value::Array(list.iter().map(computer).collect()).to_string()
}

fn state_name(s: &LinkState) -> (&'static str, u64) {
    match s {
        LinkState::Idle => ("idle", 0),
        LinkState::Dialing => ("dialing", 0),
        LinkState::Securing => ("securing", 0),
        LinkState::Open { .. } => ("open", 0),
        LinkState::Reconnecting { in_ms, .. } => ("reconnecting", *in_ms),
        LinkState::Unreachable { in_ms, .. } => ("unreachable", *in_ms),
        LinkState::Offline { .. } => ("offline", 0),
        LinkState::Refused { .. } => ("refused", 0),
        LinkState::RelayTooOld => ("relay_too_old", 0),
        LinkState::TakenOver => ("taken_over", 0),
    }
}

fn link(id: &str, v: &LinkView) -> Value {
    let (state, in_ms) = state_name(&v.state);
    let tone = match v.tone {
        Tone::Live => "live",
        Tone::Busy => "busy",
        Tone::Still => "still",
        Tone::Bad => "bad",
    };
    json!({"t": "room_state", "id": id, "state": state, "words": v.words, "tone": tone,
           "caps": v.caps, "instance": v.instance, "in_ms": in_ms})
}

pub(crate) fn json(e: &CoreEvent) -> String {
    match e {
        CoreEvent::Computers { list } => {
            json!({"t": "computers", "list": list.iter().map(computer).collect::<Vec<_>>()})
        }
        CoreEvent::RoomState { id, view } => link(id, view),
        CoreEvent::RoomMsg { id, json } => json!({"t": "room_msg", "id": id, "json": json}),
        CoreEvent::RoomRelay { id, json } => json!({"t": "room_relay", "id": id, "json": json}),
        CoreEvent::Pair { op, step } => match step {
            PairStep::Dialing => json!({"t": "pair", "op": op, "step": "dialing"}),
            PairStep::Securing => json!({"t": "pair", "op": op, "step": "securing"}),
            PairStep::CheckWords { words } => {
                json!({"t": "pair", "op": op, "step": "check_words", "words": words})
            }
            PairStep::WaitingForComputer => json!({"t": "pair", "op": op, "step": "waiting"}),
            PairStep::Done { key } => json!({"t": "pair", "op": op, "step": "done", "key": key}),
            PairStep::Failed { why } => {
                json!({"t": "pair", "op": op, "step": "failed", "why": why})
            }
        },
        CoreEvent::Answer { op, outcome } => match outcome {
            Outcome::Done => json!({"t": "answer", "op": op, "outcome": "done"}),
            Outcome::Failed(why) => {
                json!({"t": "answer", "op": op, "outcome": "failed", "why": why})
            }
            Outcome::NoAnswer => json!({"t": "answer", "op": op, "outcome": "no_answer"}),
        },
        CoreEvent::Link { .. } | CoreEvent::Host { .. } | CoreEvent::Sessions { .. } => {
            json!({"t": "other"})
        }
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn room_events_carry_id_and_payload() {
        let m = json(&CoreEvent::RoomMsg {
            id: "r1".into(),
            json: r#"{"type":"sessions"}"#.into(),
        });
        let v: Value = serde_json::from_str(&m).unwrap();
        assert_eq!(v["t"], "room_msg");
        assert_eq!(v["id"], "r1");
        assert_eq!(v["json"], r#"{"type":"sessions"}"#);
        let s = json(&CoreEvent::RoomState {
            id: "r1".into(),
            view: LinkView::new(
                LinkState::Reconnecting {
                    attempt: 1,
                    in_ms: 2500,
                },
                0,
                vec!["control".into()],
                "1:1".into(),
            ),
        });
        let v: Value = serde_json::from_str(&s).unwrap();
        assert_eq!(v["state"], "reconnecting");
        assert_eq!(v["in_ms"], 2500);
        assert_eq!(v["tone"], "busy");
    }

    #[test]
    fn pair_steps_name_themselves() {
        let v: Value = serde_json::from_str(&json(&CoreEvent::Pair {
            op: 3,
            step: PairStep::CheckWords {
                words: ["a".into(), "b".into(), "c".into(), "d".into()],
            },
        }))
        .unwrap();
        assert_eq!(v["step"], "check_words");
        assert_eq!(v["words"][3], "d");
    }
}
