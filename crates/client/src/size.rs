//! This device's size — text size and interface size (DESIGN.md, "Size"). The page owns it
//! (`js/ui_size.js`, which applies it before anything renders and writes it to the computer as
//! `device { size { … } }`); this mirrors it for the Appearance page.

use dioxus::prelude::*;

use crate::bridge;

/// The interface sizes, smallest first, with what the page calls them.
pub const INTERFACES: &[(&str, &str)] = &[
    ("compact", "Compact"),
    ("cozy", "Cozy"),
    ("comfortable", "Comfy"),
    ("large", "Large"),
];
pub const TEXT_MIN: u32 = 85;
pub const TEXT_MAX: u32 = 150;
pub const TEXT_STEP: u32 = 5;

#[derive(Clone, Debug, PartialEq)]
pub struct Size {
    /// Percent, [`TEXT_MIN`]–[`TEXT_MAX`].
    pub text: u32,
    /// Text follows the device's own size.
    pub follow: bool,
    pub interface: String,
    /// The phone's font scale (Android app); `None` in a browser.
    pub device: Option<f64>,
}

impl Default for Size {
    fn default() -> Self {
        Self {
            text: 100,
            follow: false,
            interface: "cozy".into(),
            device: None,
        }
    }
}

impl Size {
    /// The text size in force, in percent — the device's when following it (a browser's default
    /// is 100% as far as the page can tell).
    pub fn effective_text(&self) -> u32 {
        if self.follow {
            (self.device.unwrap_or(1.0) * 100.0).round() as u32
        } else {
            self.text
        }
    }
}

pub static SIZE: GlobalSignal<Size> = Signal::global(Size::default);

/// `{type:"sizePrefs", …}` from the bridge.
pub fn on_event(msg: &serde_json::Value) {
    let next = Size {
        text: msg
            .get("text")
            .and_then(|v| v.as_u64())
            .map_or(100, |v| v as u32),
        follow: msg.get("follow").and_then(|v| v.as_bool()).unwrap_or(false),
        interface: msg
            .get("interface")
            .and_then(|v| v.as_str())
            .unwrap_or("cozy")
            .to_string(),
        device: msg.get("device").and_then(|v| v.as_f64()),
    };
    if *SIZE.peek() != next {
        *SIZE.write() = next;
    }
}

/// Change one field: `text` (a number), `follow` (a bool) or `interface` (a word).
pub fn set(field: &str, value: serde_json::Value) {
    bridge::call(format!(
        "window.__wado.uiSize.set({}, {});",
        bridge::js(&field),
        bridge::js(&value)
    ));
}

/// One step up or down the text scale, held inside it.
pub fn step(text: u32, up: bool) -> u32 {
    let t = if up {
        text + TEXT_STEP
    } else {
        text.saturating_sub(TEXT_STEP)
    };
    t.clamp(TEXT_MIN, TEXT_MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_stay_on_the_scale() {
        assert_eq!(step(100, true), 105);
        assert_eq!(step(100, false), 95);
        assert_eq!(step(TEXT_MAX, true), TEXT_MAX);
        assert_eq!(step(TEXT_MIN, false), TEXT_MIN);
    }

    #[test]
    fn following_reads_the_devices_own_size() {
        let mut s = Size {
            text: 120,
            ..Size::default()
        };
        assert_eq!(s.effective_text(), 120);
        s.follow = true;
        assert_eq!(s.effective_text(), 100, "a browser follows its own default");
        s.device = Some(1.3);
        assert_eq!(s.effective_text(), 130);
    }
}
