//! Workspaces, through Hyprland: the previous and next one in use, a numbered one, and moving the
//! focused window to a numbered one.

use super::{Action, Does, Need};

pub fn all() -> Vec<Action> {
    let hypr = |args: &[&str]| {
        Does::Hypr(
            ["dispatch"]
                .iter()
                .chain(args)
                .map(|s| s.to_string())
                .collect(),
        )
    };
    let mut out = vec![
        // `e±1`: the next workspace that has windows, so a swipe never lands on an empty one.
        Action::new(
            "workspace-prev",
            "Previous workspace",
            "Workspace",
            "left",
            vec![Need::Hyprland],
            hypr(&["workspace", "e-1"]),
        ),
        Action::new(
            "workspace-next",
            "Next workspace",
            "Workspace",
            "right",
            vec![Need::Hyprland],
            hypr(&["workspace", "e+1"]),
        ),
    ];
    for n in 1..=9 {
        let n = n.to_string();
        out.push(Action::new(
            &format!("workspace-{n}"),
            &format!("Workspace {n}"),
            "Workspace",
            "monitor",
            vec![Need::Hyprland],
            hypr(&["workspace", &n]),
        ));
    }
    for n in 1..=9 {
        let n = n.to_string();
        out.push(Action::new(
            &format!("move-to-workspace-{n}"),
            &format!("Move window to workspace {n}"),
            "Workspace",
            "move",
            vec![Need::Hyprland],
            hypr(&["movetoworkspace", &n]),
        ));
    }
    out
}
