//! The daemon's config as one viewer sees it — what the control centre needs to grey out a
//! capped control, restore this device's settings, and say when the file is broken.

use serde::{Deserialize, Serialize};

use crate::EncoderBackend;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ConfigState {
    #[serde(default)]
    pub limits: HostLimits,
    /// Input settings the host pins; a set field overrides the client's own.
    #[serde(default)]
    pub input: PinnedInput,
    /// This device's saved settings blob (`device "<key>" { prefs … }`), if it has one.
    #[serde(default)]
    pub prefs: Option<String>,
    /// This device may change privileged settings.
    #[serde(default)]
    pub owner: bool,
    /// The file failed its last reload — `config.kdl:14:9: …`. The daemon is still running on
    /// the last good config.
    #[serde(default)]
    pub error: Option<String>,
    /// Sections changed in the file that only take effect after a daemon restart.
    #[serde(default)]
    pub restart: Vec<String>,
    /// What three-finger swipes do: `"swipe-3-up"` → `"app-drawer"`.
    #[serde(default)]
    pub gestures: std::collections::BTreeMap<String, String>,
    /// Keyboard shortcuts the compositor keeps: `"Mod+Q"` → `"close-window"`, and what `Mod` is.
    #[serde(default)]
    pub binds: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub bind_mod: String,
    /// How many window rules config.kdl has.
    #[serde(default)]
    pub window_rules: usize,
    /// Shells are allowed on this host.
    #[serde(default = "yes")]
    pub shells: bool,
    /// This device's trackpad layout (`device "<key>" { trackpad { … } }`), every field filled;
    /// `None` when the file has no block for it, and the phone keeps its own copy.
    #[serde(default)]
    pub trackpad: Option<TrackpadPrefs>,
    /// This device's console preferences (`device "<key>" { console { … } }`), every field
    /// filled; `None` when the file has no block for it, and the device keeps its own copy.
    #[serde(default)]
    pub console: Option<ConsolePrefs>,
    /// This device's text and interface size (`device "<key>" { size { … } }`), every field
    /// filled; `None` when the file has no block for it, and the device keeps its own copy.
    #[serde(default)]
    pub size: Option<SizePrefs>,
    /// This device's gamepad (`device "<key>" { gamepad { … } }`), every field filled; `None`
    /// when the file has no block for it, and the device keeps its own copy.
    #[serde(default)]
    pub gamepad: Option<GamepadPrefs>,
    /// The keys above this device's keyboard (`device "<key>" { keyboard { … } }`); `None` when
    /// the file has no block for it.
    #[serde(default)]
    pub keyboard: Option<KeyboardPrefs>,
    /// What this daemon can do that an older one cannot, by name — `pad-gamepad`: the pad link
    /// carries a gamepad. Empty from a daemon older than the list, which can do none of them.
    #[serde(default)]
    pub caps: Vec<String>,
    /// What this device may do here, so a page can say what is missing — and the exact command
    /// that allows it — instead of offering a control the daemon will refuse. `None`: a daemon
    /// from before this was sent; the client then offers everything and lets the daemon say no.
    #[serde(default)]
    pub grants: Option<DeviceGrants>,
}

/// One device's grants as `wado allow` names them (`crates/server/src/gate/grants.rs`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceGrants {
    /// This device as it must be typed after `wado allow` — quoted when it has spaces.
    pub name: String,
    /// `none`, `ro` or `rw`.
    pub files: String,
    pub shells: bool,
    pub settings: bool,
    pub host: bool,
    pub input: bool,
    pub clipboard: bool,
    /// Join another device's session, and take one over (`2026-10-30`). Absent from an older
    /// daemon, which cannot do either.
    #[serde(default)]
    pub join: bool,
    #[serde(default)]
    pub takeover: bool,
    /// Its key was proven with a pairing code (QR or typed) rather than carried over from before
    /// pairing. Sharing the clipboard automatically is on by default only for such a device.
    #[serde(default)]
    pub paired: bool,
}

/// One device's trackpad layout, as `wado_config::schema::DeviceTrackpad` types it. The phone
/// changes a field with `ConfigSet` on `device.<its key>.trackpad.<field>`; `quick` is sent as a
/// JSON list (`["fullscreen","play-pause"]`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrackpadPrefs {
    pub landscape: bool,
    pub fullscreen: bool,
    pub flip: bool,
    /// `keys`, `actions` or `none`.
    pub rail: String,
    /// Action ids, in order.
    pub quick: Vec<String>,
    /// `edge-left` / `edge-right` / `four-tap` / `two-hold` → an action id. Absent is off.
    pub gestures: std::collections::BTreeMap<String, String>,
    /// The keyboard opens with the pad.
    pub keyboard: bool,
    /// Pointer speed, 0.5–2.0: how far the pointer travels for the same slide. Absent from an
    /// older daemon, which always meant 1.
    #[serde(default = "one")]
    pub speed: f64,
    /// Two-finger scroll speed, 0.25–2.0.
    #[serde(default = "one")]
    pub scroll_speed: f64,
    /// Natural scrolling; `None` leaves the computer's own setting.
    #[serde(default)]
    pub natural: Option<bool>,
    /// Tap to click.
    #[serde(default = "yes")]
    pub tap: bool,
    /// Tap, then drag.
    #[serde(default = "yes")]
    pub tap_drag: bool,
    /// `off`, `briefly` or `until-tap`.
    #[serde(default = "off")]
    pub drag_lock: String,
    /// The phone's press bar.
    #[serde(default)]
    pub click_bar: bool,
    /// The phone ignores resting thumbs.
    #[serde(default = "yes")]
    pub palm: bool,
    /// The phone draws finger marks.
    #[serde(default = "yes")]
    pub marks: bool,
    /// `short`, `medium` or `long`: wado's own phone gestures.
    #[serde(default = "medium")]
    pub gesture_distance: String,
}

fn off() -> String {
    "off".into()
}

fn medium() -> String {
    "medium".into()
}

fn one() -> f64 {
    1.0
}

impl Default for TrackpadPrefs {
    fn default() -> Self {
        Self {
            landscape: false,
            fullscreen: false,
            flip: false,
            rail: String::new(),
            quick: Vec::new(),
            gestures: Default::default(),
            keyboard: false,
            speed: 1.0,
            scroll_speed: 1.0,
            natural: None,
            tap: true,
            tap_drag: true,
            drag_lock: off(),
            click_bar: false,
            palm: true,
            marks: true,
            gesture_distance: medium(),
        }
    }
}

/// One device's gamepad, as `wado_config::schema::DeviceGamepad` types it. The device changes a
/// field with `ConfigSet` on `device.<its key>.gamepad.<field>`; a saved layout is
/// `device.<its key>.gamepad.mine.<id>` set to `["name","base","{json}"]`, or to nothing to delete.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GamepadPrefs {
    /// `pad` (a real controller) or `keys` (keys & mouse, in a session).
    pub drives: String,
    /// The layout in a session and on the console's deck: a built-in id or a saved one's.
    pub layout: String,
    /// Away from a session: `same`, or a layout id.
    pub away: String,
    /// `off`, `light`, `medium` or `strong` — for the touchpad and keyboard too.
    pub haptics: String,
    /// `right` or `left` — for the touchpad and keyboard too.
    pub hand: String,
    /// The device's own layouts, by id.
    pub mine: Vec<SavedLayout>,
}

/// One saved gamepad layout: the built-in it was made from and its changes, as the client's
/// layout JSON (the daemon does not read inside it).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedLayout {
    pub id: String,
    pub name: String,
    pub base: String,
    pub json: String,
}

/// The keys above one device's keyboard, as `wado_config::schema::DeviceKeyboard` types it,
/// in order. Changed with `ConfigSet` on `device.<its key>.keyboard.keys` as a JSON list.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyboardPrefs {
    pub keys: Vec<String>,
}

/// How one device opens a session, as `wado_config::schema::DeviceConsole` types it. The device
/// changes a field with `ConfigSet` on `device.<its key>.console.<field>`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConsolePrefs {
    /// `console` or `full`.
    pub open: String,
    /// `auto`, `pad`, `keys`, `quick` or `gamepad`.
    pub face: String,
}

/// How big one device draws the client, as `wado_config::schema::DeviceSize` types it. The
/// device changes a field with `ConfigSet` on `device.<its key>.size.<field>`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SizePrefs {
    /// Text size in percent, 85–150 in steps of 5.
    pub text: u32,
    /// Text follows the device's own font size instead of `text`.
    pub follow: bool,
    /// `compact`, `cozy`, `comfortable` or `large`.
    pub interface: String,
}

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostLimits {
    pub max_fps: Option<u32>,
    pub max_bitrate: Option<u32>,
    pub max_width: Option<u32>,
    pub max_height: Option<u32>,
    /// Set when the host forces one encoder for everyone.
    pub encoder: Option<EncoderBackend>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PinnedInput {
    pub repeat_rate: Option<i32>,
    pub repeat_delay: Option<i32>,
    pub focus_follows_pointer: Option<bool>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_older_daemon_reads_as_no_caps_no_gamepad_and_speed_one() {
        let s: ConfigState = serde_json::from_str(
            r#"{"trackpad":{"landscape":true,"fullscreen":false,"flip":false,"rail":"keys","quick":[],"gestures":{},"keyboard":false}}"#,
        )
        .unwrap();
        assert!(s.caps.is_empty());
        assert!(s.gamepad.is_none() && s.keyboard.is_none());
        let t = s.trackpad.unwrap();
        assert_eq!((t.speed, t.scroll_speed, t.natural), (1.0, 1.0, None));
        assert!(t.tap && t.tap_drag && t.palm && t.marks && !t.click_bar);
        assert_eq!(
            (t.drag_lock.as_str(), t.gesture_distance.as_str()),
            ("off", "medium")
        );
    }

    #[test]
    fn gamepad_prefs_go_out_as_plain_fields() {
        let g = GamepadPrefs {
            drives: "pad".into(),
            layout: "mine-1".into(),
            away: "same".into(),
            haptics: "medium".into(),
            hand: "right".into(),
            mine: vec![SavedLayout {
                id: "mine-1".into(),
                name: "Couch".into(),
                base: "racing".into(),
                json: "{}".into(),
            }],
        };
        assert_eq!(
            serde_json::to_value(&g).unwrap(),
            serde_json::json!({"drives":"pad","layout":"mine-1","away":"same","haptics":"medium",
                "hand":"right","mine":[{"id":"mine-1","name":"Couch","base":"racing","json":"{}"}]})
        );
    }
}
