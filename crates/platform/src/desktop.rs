//! The desktop the daemon runs in: its windows and workspaces, opening apps on it, capturing it.

use std::path::PathBuf;

use crate::{Fut, Res};

/// Left or right.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Dir {
    Left,
    Right,
}

/// One thing to do to the focused window or the workspaces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WindowOp {
    Fullscreen,
    Close,
    ToggleFloat,
    Pin,
    Focus(Dir),
    /// The previous / next workspace that has windows — a swipe never lands on an empty one.
    WorkspaceStep(Dir),
    /// Workspace `n` (1..=9).
    Workspace(u8),
    /// The focused window to workspace `n` (1..=9).
    MoveToWorkspace(u8),
}

/// The window manager.
pub trait WindowManager: Send + Sync {
    fn run(&self, op: WindowOp) -> Fut<'_, Res<()>>;
}

/// An app the launcher can open.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum App {
    Terminal,
    Browser,
    Files,
}

/// Opening apps on the desktop. Started and left to run: an app outlives the tap that opened it.
pub trait Launcher: Send + Sync {
    /// `app` can be opened here.
    fn can(&self, app: App) -> bool;
    fn open(&self, app: App) -> Res<()>;
}

/// Screenshots of the desktop, saved as PNG.
pub trait Screenshot: Send + Sync {
    /// The person can choose an area (else only the whole screen).
    fn can_region(&self) -> bool;
    /// Capture the screen — or an area the person chooses on it — into `to`.
    fn shoot(&self, region: bool, to: PathBuf) -> Fut<'_, Res<()>>;
}
