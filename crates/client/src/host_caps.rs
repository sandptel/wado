//! What a computer can do, by name — `ConfigState::platform` and `host_caps`
//! (`wado_protocol::caps::CapSet`). The one place the rules live; components ask [`cap_state`].
//!
//! A Mac is never taken for Linux: on `platform == "macos"` a feature the daemon marks `later` is
//! shown dimmed with a [`SOON`] badge and its reason on a tap, and one it marks missing for good
//! is hidden and listed on the computer's "On this Mac" card. Anything else — a Linux daemon, one
//! older than these fields, a name in neither list — is on: the UI is exactly what it always was.

use dioxus::prelude::*;
use wado_protocol::ConfigState;

use crate::state::Ui;

/// The badge on a feature that is planned for the Mac.
pub const SOON: &str = "Coming to macOS";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Cap {
    On,
    /// Coming to this OS: shown dimmed, the reason on a tap.
    Later(String),
    /// Not here: hidden, the reason on "On this Mac".
    Off(String),
}

impl Cap {
    pub fn on(&self) -> bool {
        *self == Cap::On
    }

    pub fn why(&self) -> &str {
        match self {
            Cap::On => "",
            Cap::Later(w) | Cap::Off(w) => w,
        }
    }
}

/// The computer is a Mac.
pub fn mac(c: &ConfigState) -> bool {
    c.platform == "macos"
}

pub fn cap_state(c: &ConfigState, name: &str) -> Cap {
    if !mac(c) {
        return Cap::On;
    }
    match c.host_caps.off.iter().find(|o| o.cap == name) {
        Some(o) if o.later => Cap::Later(o.why.clone()),
        Some(o) => Cap::Off(o.why.clone()),
        None => Cap::On,
    }
}

/// `name` on the selected computer.
pub fn here(ui: Ui, name: &str) -> Cap {
    cap_state(&ui.live.host.read(), name)
}

/// `name` is missing on the selected computer for good: what needs it is not shown.
pub fn gone(ui: Ui, name: &str) -> bool {
    matches!(here(ui, name), Cap::Off(_))
}

/// Say why, over the page.
pub fn tell(ui: Ui, why: &str) {
    let mut note = ui.live.note;
    note.set(why.to_string());
}

/// `name` is not on the selected computer: say why and answer true — the guard an action that
/// needs it runs first, so no button, shortcut or gesture can get past it.
pub fn refused(ui: Ui, name: &str) -> bool {
    let c = here(ui, name);
    if !c.on() {
        tell(ui, c.why());
    }
    !c.on()
}

/// A feature's name in plain words, for the "On this Mac" card.
pub fn label(name: &str) -> &str {
    match name {
        "touch" => "Touchpad",
        "keys" => "Keyboard keys",
        "gamepad" => "Gamepad",
        "text" => "Typing text",
        "audio" => "Volume and outputs",
        "viewer_sink" => "Its sound on this phone",
        "media" => "Media controls",
        "brightness" => "Brightness",
        "wifi" => "Wi-Fi",
        "bluetooth" => "Bluetooth",
        "power_inhibit" => "Keep awake",
        "system_power" => "Lock and sleep",
        "clipboard" => "Clipboard",
        "clip_history" => "Clipboard history",
        "window_manager" => "Windows and Spaces",
        "launcher" => "Opening apps",
        "screenshot" => "Screenshots",
        "sys_info" => "About the computer",
        "trash" => "Trash",
        "thumbnailer" => "Thumbnails",
        "pty" => "Shells",
        "session" => "Screen sessions",
        "sound" => "Sound in a session",
        "notifications" => "Notifications",
        "menus" => "App menus",
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(json: &str) -> ConfigState {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn an_older_or_linux_daemon_has_everything_on() {
        let old = config("{}");
        assert!(!mac(&old));
        assert_eq!(cap_state(&old, "session"), Cap::On);
        // Linux sends its own missing pieces; the UI shows them as it always has.
        let linux = config(
            r#"{"platform":"linux","host_caps":{"on":[],"off":[{"cap":"wifi","why":"no NetworkManager"}]}}"#,
        );
        assert_eq!(cap_state(&linux, "wifi"), Cap::On);
    }

    #[test]
    fn a_mac_reads_later_off_and_unknown() {
        let c = config(
            r#"{"platform":"macos","host_caps":{"on":["audio"],"off":[
                {"cap":"session","why":"A session is coming to macOS.","later":true},
                {"cap":"wifi","why":"Not on macOS: Control Centre."},
                {"cap":"touch","why":"Allow wado in System Settings › Privacy & Security › Accessibility"}]}}"#,
        );
        assert!(mac(&c));
        assert_eq!(
            cap_state(&c, "session"),
            Cap::Later("A session is coming to macOS.".into())
        );
        assert_eq!(
            cap_state(&c, "wifi"),
            Cap::Off("Not on macOS: Control Centre.".into())
        );
        assert!(cap_state(&c, "touch").why().starts_with("Allow wado"));
        assert_eq!(cap_state(&c, "audio"), Cap::On);
        assert_eq!(cap_state(&c, "teleport"), Cap::On);
    }
}
