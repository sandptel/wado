//! The focused window, through the window manager: fullscreen, close, float, pin, move focus.

use wado_platform::Capability;
use wado_platform::desktop::{Dir, WindowOp};

use super::{Action, Does, Need};

pub fn all() -> Vec<Action> {
    [
        (
            "fullscreen",
            "Window full screen",
            "max",
            WindowOp::Fullscreen,
        ),
        ("close-window", "Close window", "x", WindowOp::Close),
        (
            "toggle-float",
            "Float window",
            "layers",
            WindowOp::ToggleFloat,
        ),
        ("pin", "Pin window", "pin", WindowOp::Pin),
        (
            "focus-left",
            "Focus left",
            "left",
            WindowOp::Focus(Dir::Left),
        ),
        (
            "focus-right",
            "Focus right",
            "right",
            WindowOp::Focus(Dir::Right),
        ),
    ]
    .into_iter()
    .map(|(id, name, icon, op)| {
        Action::new(
            id,
            name,
            "Window",
            icon,
            vec![Need::Cap(Capability::WindowManager)],
            Does::Window(op),
        )
    })
    .collect()
}
