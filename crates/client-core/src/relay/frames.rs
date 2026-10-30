//! What an inbound frame is, before anything acts on it: the relay's own handshake, the
//! envelope's plaintext steps, a sealed frame from the computer, or something else. Anything else
//! in the clear claims to be the computer but is not sealed by it, so it is never acted on — and
//! an unknown type is `Other`, never an error (forward compatibility).

use wado_protocol::envelope::{E2eMsg, SEALED_PREFIX};
use wado_protocol::relay_wire::WireMsg;

#[derive(Debug, Clone, PartialEq)]
pub enum Frame {
    Wire(WireMsg),
    E2e(E2eMsg),
    Sealed,
    Other,
}

pub fn classify(text: &str) -> Frame {
    if text.starts_with(SEALED_PREFIX) {
        return Frame::Sealed;
    }
    if let Ok(w) = serde_json::from_str::<WireMsg>(text) {
        return Frame::Wire(w);
    }
    match serde_json::from_str::<E2eMsg>(text) {
        Ok(E2eMsg::Sealed { .. }) => Frame::Sealed,
        Ok(m) => Frame::E2e(m),
        Err(_) => Frame::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn join_accepted_is_wire() {
        let f = classify(
            r#"{"type":"join_accepted","remote_id":"1","room_id":"r","caps":["control"]}"#,
        );
        assert!(
            matches!(f, Frame::Wire(WireMsg::JoinAccepted { .. })),
            "{f:?}"
        );
        assert_eq!(classify(r#"{"type":"ping"}"#), Frame::Wire(WireMsg::Ping));
    }

    #[test]
    fn e2e_reply_is_e2e() {
        let f = classify(r#"{"type":"e2e_reply","eph":"a","host_pk":"b","sig":"c"}"#);
        assert!(matches!(f, Frame::E2e(E2eMsg::E2eReply { .. })), "{f:?}");
        let f = classify(r#"{"type":"e2e_fail","reason":"no"}"#);
        assert!(matches!(f, Frame::E2e(E2eMsg::E2eFail { .. })), "{f:?}");
    }

    #[test]
    fn sealed_is_sealed() {
        assert_eq!(
            classify(r#"{"type":"sealed","n":0,"c":"AA"}"#),
            Frame::Sealed
        );
        // Field order is not the prefix's: still sealed.
        assert_eq!(
            classify(r#"{"n":0,"type":"sealed","c":"AA"}"#),
            Frame::Sealed
        );
    }

    #[test]
    fn unknown_type_is_other_not_error() {
        assert_eq!(
            classify(r#"{"type":"from_the_future","x":1}"#),
            Frame::Other
        );
        assert_eq!(classify("not json"), Frame::Other);
        // A computer message in the clear is not acted on.
        assert_eq!(
            classify(r#"{"type":"host_state","state":{}}"#),
            Frame::Other
        );
    }
}
