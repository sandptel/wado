//! What a control-only link may carry, and the grant each message needs — the whole of the rule,
//! with no I/O, so it can be read and tested on its own.
//!
//! A control link is a phone's widget: the computer's state and its controls, never its desktop.
//! Anything that would start, join or drive a session, open a shell, move files or touch input is
//! [`Need::NotControl`] and refused, whatever the device's grants say.

use wado_protocol::pad::PadMsg;
use wado_protocol::relay::RelayMsg;

use crate::gate::Grants;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Need {
    /// Any trusted device: reading the computer's state, or its own config.
    Trusted,
    /// Every one of these grants, as `wado allow` names them.
    Grants(&'static [&'static str]),
    /// Not a control: never on this link.
    NotControl,
}

/// The grant `msg` needs on a control link.
pub fn need(msg: &RelayMsg) -> Need {
    match msg {
        RelayMsg::HostGet { .. } | RelayMsg::ConfigGet => Need::Trusted,
        // Wi-Fi can cut the computer off: the owner's say, as on the main link.
        RelayMsg::HostDo { ack: _, action } if action.is_network() => {
            Need::Grants(&["host", "settings"])
        }
        RelayMsg::HostDo { .. } => Need::Grants(&["host"]),
        // The Clipboard widget is the whole Clipboard surface: the history, each entry's copy,
        // send, pin and delete, and following the computer's copies as they happen.
        RelayMsg::ClipList
        | RelayMsg::ClipGet { .. }
        | RelayMsg::ClipPush { .. }
        | RelayMsg::ClipPin { .. }
        | RelayMsg::ClipDelete { .. }
        | RelayMsg::ClipWatch { .. } => Need::Grants(&["clipboard"]),
        RelayMsg::PadIn {
            msg: PadMsg::Actions | PadMsg::Run { .. },
        } => Need::Grants(&["host"]),
        _ => Need::NotControl,
    }
}

/// The first of `tokens` that `g` lacks.
pub fn missing(g: &Grants, tokens: &[&'static str]) -> Option<&'static str> {
    tokens.iter().copied().find(|t| match *t {
        "host" => !g.host,
        "settings" => !g.settings,
        "clipboard" => !g.clipboard,
        "input" => !g.input,
        "shells" => !g.shells,
        "join" => !g.join,
        "takeover" => !g.takeover,
        _ => true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use wado_protocol::host::HostAction;

    #[test]
    fn controls_pass_and_the_desktop_never_does() {
        assert_eq!(need(&RelayMsg::HostGet { ack: 0 }), Need::Trusted);
        assert_eq!(need(&RelayMsg::ConfigGet), Need::Trusted);
        assert_eq!(
            need(&RelayMsg::HostDo {
                ack: 0,
                action: HostAction::Brightness { level: 0.5 }
            }),
            Need::Grants(&["host"])
        );
        assert_eq!(
            need(&RelayMsg::HostDo {
                ack: 0,
                action: HostAction::WifiKeep
            }),
            Need::Grants(&["host", "settings"])
        );
        for m in [
            r#"{"type":"clip_list"}"#,
            r#"{"type":"clip_get","id":"1"}"#,
            r#"{"type":"clip_push","mime":"text/plain","data":"x"}"#,
            r#"{"type":"clip_pin","id":"1","on":true}"#,
            r#"{"type":"clip_delete","id":"1"}"#,
            r#"{"type":"clip_watch","live":true}"#,
        ] {
            let msg: RelayMsg = serde_json::from_str(m).unwrap_or_else(|e| panic!("{m}: {e}"));
            assert_eq!(need(&msg), Need::Grants(&["clipboard"]), "{m}");
        }
        assert_eq!(
            need(&RelayMsg::PadIn {
                msg: PadMsg::Run { id: "lock".into() }
            }),
            Need::Grants(&["host"])
        );
        // Touch, keys, a session, a shell: not on a widget's link, whatever the grants.
        for m in [
            r#"{"type":"pad_in","msg":{"k":"key","key":"esc","mods":[]}}"#,
            r#"{"type":"session_launch","command":"foot"}"#,
            r#"{"type":"sessions_request"}"#,
            r#"{"type":"clipboard_set","text":"into a session"}"#,
            r#"{"type":"config_set","key":"stream.max-fps","value":"30","confirmed":false}"#,
        ] {
            let msg: RelayMsg = serde_json::from_str(m).unwrap_or_else(|e| panic!("{m}: {e}"));
            assert_eq!(need(&msg), Need::NotControl, "{m}");
        }
    }

    #[test]
    fn the_first_missing_grant_is_named() {
        let mut g = Grants::NONE;
        assert_eq!(missing(&g, &["host", "settings"]), Some("host"));
        g.host = true;
        assert_eq!(missing(&g, &["host", "settings"]), Some("settings"));
        g.settings = true;
        assert_eq!(missing(&g, &["host", "settings"]), None);
        assert_eq!(missing(&g, &["clipboard"]), Some("clipboard"));
    }
}
