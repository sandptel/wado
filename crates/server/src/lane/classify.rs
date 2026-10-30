//! Which frames may jump the lane's queue, which wait their turn, and which are not the lane's.
//!
//! Read off the frame's `type` before it is sealed (`crate::e2e::link::kind`). The rule that
//! decides a type's class: **two types whose order matters to the device share a class** —
//! `pty_exit` waits behind the `pty_output` before it, `link_test_done` behind its chunks, a
//! file's last answer behind its bytes. Within a class, order is kept.

use crate::e2e::link::kind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Class {
    /// Not lane traffic: frames for the relay itself (it never forwards them), and control rooms'
    /// frames, which their own writer has already paced.
    Free,
    /// Small and time-critical — signalling, session verbs, answers a person is waiting on.
    /// Sent at once, ahead of anything bulk, and still counted against the budget.
    Urgent,
    /// Big or streaming: shell output, transfers, the speed test, pictures in state. Paced.
    Bulk,
    /// Worth sending only when the lane is idle: the daemon's log tail. Dropped otherwise.
    Spare,
}

/// For the relay itself, or wrapped for a control room (already paced there).
const FREE: &[&str] = &[
    "register",
    "pong",
    "peer_accept",
    "peer_reject",
    "seat_hold",
    "occupancy",
    "ctl",
    "ctl_close",
];

/// What waits its turn. Everything not named here or above is urgent: a new message type is
/// small and prompt until someone measures otherwise. The speed test (`link_test_*`) is urgent on
/// purpose: it measures the path and must not measure this pacer, and it is charged, so the
/// relay's model of the link stays true (`wado_protocol::lane`).
const BULK: &[&str] = &[
    "pty_output",
    "pty_exit",
    "files_out",
    "files_chunk",
    "clip_data",
    "clip_history",
    "clipboard",
    "host_state",
    "apps_list",
    "notification",
    "claude_cards",
    "claude_dirs",
    "targets",
    "shells",
];

const SPARE: &[&str] = &["log"];

pub fn class(text: &str) -> Class {
    // A writer's own sentinel (a control room's close): after everything already queued.
    if text.starts_with('\u{0}') {
        return Class::Bulk;
    }
    let Some(k) = kind(text) else {
        // The envelope's wake-up (empty) and anything unreadable: let it through promptly.
        return Class::Urgent;
    };
    if FREE.contains(&k) {
        Class::Free
    } else if BULK.contains(&k) {
        Class::Bulk
    } else if SPARE.contains(&k) {
        Class::Spare
    } else {
        Class::Urgent
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signalling_jumps_the_queue_and_streams_wait() {
        assert_eq!(class(r#"{"type":"sdp_answer","sdp":"x"}"#), Class::Urgent);
        assert_eq!(
            class(r#"{"type":"session_started","info":{}}"#),
            Class::Urgent
        );
        assert_eq!(
            class(r#"{"type":"pad_out","msg":{"k":"pong","t":1}}"#),
            Class::Urgent
        );
        assert_eq!(
            class(r#"{"type":"pty_output","id":1,"data":"x"}"#),
            Class::Bulk
        );
        assert_eq!(
            class(r#"{"type":"files_chunk","id":1,"b":""}"#),
            Class::Bulk
        );
        assert_eq!(class(r#"{"type":"log","line":"x"}"#), Class::Spare);
        assert_eq!(class(r#"{"type":"pong"}"#), Class::Free);
        assert_eq!(
            class(r#"{"type":"ctl","room_id":"r","text":"x"}"#),
            Class::Free
        );
        assert_eq!(class(""), Class::Urgent, "the envelope's wake-up");
    }

    #[test]
    fn what_must_stay_in_order_shares_a_class() {
        for pair in [
            ("pty_output", "pty_exit"),
            ("link_test_chunk", "link_test_done"),
            ("files_chunk", "files_out"),
        ] {
            let a = class(&format!(r#"{{"type":"{}"}}"#, pair.0));
            let b = class(&format!(r#"{{"type":"{}"}}"#, pair.1));
            assert_eq!(a, b, "{pair:?}");
        }
    }
}
