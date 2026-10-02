//! Every quick tile there is: what it shows, what a tap does, which page a long-press opens.
//!
//! A table rather than a component per tile: a tile is four facts and an action, and twelve
//! near-identical components would be twelve places to get the long-press wrong.

use dioxus::prelude::*;

use crate::{
    bridge, cfg,
    state::Ui,
    ui::{gamepad, live, pages::Page},
};

/// What tapping does. Two tiles cannot be plain buttons: the keyboard must be a `label` so
/// Android raises the soft keyboard inside the gesture (see `js/osk.js`), and pointer lock must
/// carry the id `js/input_lock.js` catches in the capture phase.
#[derive(Clone, Copy)]
pub enum Act {
    Run(fn(Ui)),
    Keyboard,
    PointerLock,
}

pub struct Tile {
    pub id: &'static str,
    pub icon: &'static str,
    pub label: &'static str,
    /// `None`: a one-shot action with no on/off state.
    pub on: Option<fn(Ui) -> bool>,
    /// The subtitle when off and when on.
    pub sub: (&'static str, &'static str),
    pub act: Act,
    pub page: Option<Page>,
    /// Does nothing without a running session.
    pub needs_session: bool,
}

pub const DEFAULTS: [&str; 8] = [
    "kbd",
    "pad",
    "mouse",
    "fullscreen",
    "move",
    "touch",
    "fpslock",
    "natscroll",
];

pub fn defaults() -> Vec<String> {
    DEFAULTS.iter().map(|s| s.to_string()).collect()
}

pub fn get(id: &str) -> Option<&'static Tile> {
    ALL.iter().find(|t| t.id == id)
}

pub static ALL: &[Tile] = &[
    Tile {
        id: "kbd",
        icon: "kbd",
        label: "Keyboard",
        on: Some(|ui| (ui.live.osk_on)()),
        sub: ("Off", "On screen"),
        act: Act::Keyboard,
        page: Some(Page::Input),
        needs_session: true,
    },
    Tile {
        id: "pad",
        icon: "pad",
        label: "Gamepad",
        on: Some(|ui| (ui.set.pad_on)()),
        sub: ("Hidden", "Overlay on"),
        act: Act::Run(|ui| {
            let mut s = ui.set;
            s.pad_on.set(!(s.pad_on)());
            gamepad::apply(ui);
        }),
        page: Some(Page::Gamepad),
        needs_session: true,
    },
    Tile {
        id: "mouse",
        icon: "target",
        label: "Take mouse",
        on: Some(|ui| (ui.live.pointer_lock)()),
        sub: ("Free", "Captured · Esc"),
        act: Act::PointerLock,
        page: None,
        needs_session: true,
    },
    Tile {
        id: "fullscreen",
        icon: "max",
        label: "Fullscreen",
        on: None,
        sub: ("Fill this screen", ""),
        act: Act::Run(|ui| {
            // The session's own size, so the orientation lock matches the output.
            let c = cfg::build(ui);
            bridge::call(format!(
                "window.__wado.toggleFullscreen({}, {});",
                c.width, c.height
            ));
        }),
        page: None,
        needs_session: false,
    },
    Tile {
        id: "move",
        icon: "move",
        label: "Move windows",
        on: Some(|ui| (ui.set.move_mode)()),
        sub: ("Drag = input", "Drag = move"),
        act: Act::Run(|ui| {
            let mut s = ui.set;
            s.move_mode.set(!(s.move_mode)());
            live::apply(ui);
        }),
        page: Some(Page::Input),
        needs_session: false,
    },
    Tile {
        id: "touch",
        icon: "hand",
        label: "Raw touch",
        on: Some(|ui| (ui.set.touch_mode)() == "touch"),
        sub: ("Gestures", "Multi-touch"),
        act: Act::Run(|ui| {
            let mut s = ui.set;
            let next = if (s.touch_mode)() == "touch" {
                "pointer"
            } else {
                "touch"
            };
            s.touch_mode.set(next.into());
            live::apply(ui);
        }),
        page: Some(Page::Input),
        needs_session: false,
    },
    Tile {
        id: "fpslock",
        icon: "lock",
        label: "Lock FPS",
        on: Some(|ui| (ui.set.fps_lock)()),
        sub: ("Adaptive", "Locked"),
        act: Act::Run(|ui| {
            let mut s = ui.set;
            let on = !(s.fps_lock)();
            s.fps_lock.set(on);
            bridge::call(format!("window.__wado.setFpsLock({on});"));
        }),
        page: Some(Page::Display),
        needs_session: false,
    },
    Tile {
        id: "natscroll",
        icon: "scroll",
        label: "Natural scroll",
        on: Some(|ui| (ui.set.natural_scroll)()),
        sub: ("Off", "On"),
        act: Act::Run(|ui| {
            let mut s = ui.set;
            s.natural_scroll.set(!(s.natural_scroll)());
            live::apply(ui);
        }),
        page: Some(Page::Input),
        needs_session: false,
    },
    Tile {
        id: "stats",
        icon: "eye",
        label: "Readouts",
        on: Some(|ui| (ui.set.debug_master)()),
        sub: ("Hidden", "Over the picture"),
        act: Act::Run(|ui| {
            let mut s = ui.set;
            s.debug_master.set(!(s.debug_master)());
            crate::debug::apply(ui);
        }),
        page: Some(Page::Diagnostics),
        needs_session: false,
    },
    Tile {
        id: "awake",
        icon: "sun",
        label: "Keep awake",
        on: Some(|ui| (ui.set.keep_awake)()),
        sub: ("Screen may sleep", "Screen stays on"),
        act: Act::Run(|ui| {
            let mut s = ui.set;
            s.keep_awake.set(!(s.keep_awake)());
            live::apply(ui);
        }),
        page: None,
        needs_session: false,
    },
    Tile {
        id: "lens",
        icon: "search",
        label: "Lens",
        on: Some(|ui| (ui.set.lens_auto)()),
        sub: ("Taps click", "Unsure taps magnify"),
        act: Act::Run(|ui| {
            let mut s = ui.set;
            s.lens_auto.set(!(s.lens_auto)());
            live::apply(ui);
        }),
        page: Some(Page::Input),
        needs_session: false,
    },
    Tile {
        id: "clipboard",
        icon: "clip",
        label: "Paste here",
        on: None,
        sub: ("Phone clipboard → session", ""),
        act: Act::Run(|_| bridge::call("window.__wado.clipboardFromPhone();".to_string())),
        page: None,
        needs_session: true,
    },
    Tile {
        id: "dnd",
        icon: "bell",
        label: "Do not disturb",
        on: Some(|ui| (ui.set.dnd)()),
        sub: ("Notifications pop up", "Shade only"),
        act: Act::Run(|ui| {
            let mut s = ui.set;
            s.dnd.set(!(s.dnd)());
        }),
        page: None,
        needs_session: false,
    },
    Tile {
        id: "screenshot",
        icon: "camera",
        label: "Screenshot",
        on: None,
        sub: ("Save the picture", ""),
        act: Act::Run(|ui| {
            let mut l = ui.live;
            l.cc_open.set(false);
            bridge::call("setTimeout(() => window.__wado.screenshot(), 450);".to_string());
        }),
        page: None,
        needs_session: true,
    },
    Tile {
        id: "record",
        icon: "record",
        label: "Record",
        on: Some(|ui| (ui.live.recording)()),
        sub: ("Save a video", "Recording · tap to stop"),
        act: Act::Run(|_| bridge::call("window.__wado.recordToggle();".to_string())),
        page: None,
        needs_session: true,
    },
    Tile {
        id: "resync",
        icon: "refresh",
        label: "Resync",
        on: None,
        sub: ("New connection, same apps", ""),
        act: Act::Run(|_| bridge::call("window.__wado.resync();".to_string())),
        page: None,
        needs_session: true,
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique_and_defaults_exist() {
        for (i, t) in ALL.iter().enumerate() {
            assert!(ALL[i + 1..].iter().all(|u| u.id != t.id), "{}", t.id);
        }
        assert!(DEFAULTS.iter().all(|d| get(d).is_some()));
    }
}
