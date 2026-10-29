//! Workspaces, through the window manager: the previous and next one in use, a numbered one, and
//! moving the focused window to a numbered one.

use wado_platform::Capability;
use wado_platform::desktop::{Dir, WindowOp};

use super::{Action, Does, Need};

pub fn all() -> Vec<Action> {
    let wm = || vec![Need::Cap(Capability::WindowManager)];
    let mut out = vec![
        Action::new(
            "workspace-prev",
            "Previous workspace",
            "Workspace",
            "left",
            wm(),
            Does::Window(WindowOp::WorkspaceStep(Dir::Left)),
        ),
        Action::new(
            "workspace-next",
            "Next workspace",
            "Workspace",
            "right",
            wm(),
            Does::Window(WindowOp::WorkspaceStep(Dir::Right)),
        ),
    ];
    for n in 1..=9u8 {
        out.push(Action::new(
            &format!("workspace-{n}"),
            &format!("Workspace {n}"),
            "Workspace",
            "monitor",
            wm(),
            Does::Window(WindowOp::Workspace(n)),
        ));
    }
    for n in 1..=9u8 {
        out.push(Action::new(
            &format!("move-to-workspace-{n}"),
            &format!("Move window to workspace {n}"),
            "Workspace",
            "move",
            wm(),
            Does::Window(WindowOp::MoveToWorkspace(n)),
        ));
    }
    out
}
