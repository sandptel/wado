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
    /// `app` can be opened here — or why not, in words a person can act on.
    fn can(&self, app: App) -> Res<()>;
    fn open(&self, app: App) -> Res<()>;
}

/// The desktop's own apps, with no session: what is installed and running (and in front, hidden,
/// in its dock), each one's icon, and opening, showing, hiding and quitting them there.
///
/// Provided on macOS (`wado-host-macos` `apps.rs`). TODO(linux): provide it from
/// `crate::platform::linux` for a desktop wado did not start — installed apps from the XDG entries
/// `crates/server/src/apps` already reads; running/front/hidden from the compositor's toplevels
/// (wlr-foreign-toplevel or the desktop's own IPC: hyprctl, swaymsg, GNOME's Shell D-Bus);
/// `dock` from the desktop's favourites (GNOME `org.gnome.shell favorite-apps`, KDE task manager);
/// open via `gtk-launch`/`gio launch`; hide/quit via the toplevel protocol; `url_apps` from
/// `gio mime x-scheme-handler/https` (`xdg-mime query default` for the default); `open_url` via
/// `gio open` / `gtk-launch <app> <url>`. The clients need no change — they follow `desk_apps`.
pub trait DeskApps: Send + Sync {
    /// Installed and running apps, without icons; `exec` names each one for [`DeskApps::act`].
    fn list(&self) -> Fut<'_, Res<Vec<wado_protocol::AppEntry>>>;
    /// Icons as `data:` URIs, by `exec`; an app with none is left out.
    fn icons(&self, execs: Vec<String>) -> Fut<'_, std::collections::BTreeMap<String, String>>;
    fn act(&self, exec: String, action: wado_protocol::AppAction) -> Fut<'_, Res<()>>;
    /// The apps that can open `url`, and which one does by default (its `exec`).
    fn url_apps(&self, url: String)
    -> Fut<'_, Res<(Option<String>, Vec<wado_protocol::AppEntry>)>>;
    /// Open `url` in the app `exec`, or the default one.
    fn open_url(&self, url: String, exec: Option<String>) -> Fut<'_, Res<()>>;
}

/// Screenshots of the desktop, saved as PNG.
pub trait Screenshot: Send + Sync {
    /// The person can choose an area (else only the whole screen) — or why not.
    fn can_region(&self) -> Res<()>;
    /// Capture the screen — or an area the person chooses on it — into `to`.
    fn shoot(&self, region: bool, to: PathBuf) -> Fut<'_, Res<()>>;
}
