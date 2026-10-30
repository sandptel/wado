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
        RelayMsg::HostGet | RelayMsg::ConfigGet => Need::Trusted,
        // Wi-Fi can cut the computer off: the owner's say, as on the main link.
        RelayMsg::HostDo { action } if action.is_network() => Need::Grants(&["host", "settings"]),
        RelayMsg::HostDo { .. } => Need::Grants(&["host"]),
        RelayMsg::ClipList | RelayMsg::ClipGet { .. } | RelayMsg::ClipPush { .. } => {
            Need::Grants(&["clipboard"])
        }
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
        assert_eq!(need(&RelayMsg::HostGet), Need::Trusted);
        assert_eq!(need(&RelayMsg::ConfigGet), Need::Trusted);
        assert_eq!(
            need(&RelayMsg::HostDo {
                action: HostAction::Brightness { level: 0.5 }
            }),
            Need::Grants(&["host"])
        );
        assert_eq!(
            need(&RelayMsg::HostDo {
                action: HostAction::WifiKeep
            }),
            Need::Grants(&["host", "settings"])
        );
        assert_eq!(need(&RelayMsg::ClipList), Need::Grants(&["clipboard"]));
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
            r#"{"type":"clip_delete","id":"1"}"#,
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
