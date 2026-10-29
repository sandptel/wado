//! The focused window, through Hyprland: fullscreen, close, float, pin, move focus.

use super::{Action, Does, Need};

pub fn all() -> Vec<Action> {
    [
        ("fullscreen", "Fullscreen", "max", &["fullscreen"][..]),
        ("close-window", "Close window", "x", &["killactive"]),
        (
            "toggle-float",
            "Float window",
            "layers",
            &["togglefloating"],
        ),
        ("pin", "Pin window", "pin", &["pin"]),
        ("focus-left", "Focus left", "left", &["movefocus", "l"]),
        ("focus-right", "Focus right", "right", &["movefocus", "r"]),
    ]
    .into_iter()
    .map(|(id, name, icon, dispatch)| {
        let args = ["dispatch"].iter().chain(dispatch).map(|s| s.to_string());
        Action::new(
            id,
            name,
            "Window",
            icon,
            vec![Need::Hyprland],
            Does::Hypr(args.collect()),
        )
    })
    .collect()
}
