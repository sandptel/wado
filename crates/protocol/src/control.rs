//! Session control: the things a client asks a *running* session to do.
//!
//! One enum rather than a route or a message per action. The alternative — which this
//! replaced — was `POST /session/launch` plus a sibling for each new verb, each of which also
//! needed a matching relay message; four window actions would have meant eight new plumbing
//! sites. Adding a verb is now a variant here and a match arm in the compositor.
//!
//! Note the asymmetry with [`crate::relay::RelayMsg`], which is deliberate: `RelayMsg` is a
//! flat, forward-verbatim namespace where one variant per action is already the idiom and
//! costs the relay nothing, so it keeps `SessionLaunch` and gains `SessionWindow` as a peer.
//! The consolidation is worth it for HTTP, where each verb would otherwise be a whole route.

use serde::{Deserialize, Serialize};

/// A request against the running session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SessionControl {
    /// Spawn a command into the session. Repeatable.
    Launch { command: String },
    /// Act on the focused window.
    Window(WindowAction),
}

/// What to do to the window that currently holds keyboard focus.
///
/// **Focused-window only, with no window list.** The compositor already knows what is focused,
/// so nothing has to be tracked, serialised, or pushed to the client over two transports.
/// The cost is named rather than hidden: without titles, [`WindowAction::CycleFocus`] is a
/// blind rotation — you press until the window you wanted appears, and each press costs a
/// render round trip to see what you got. Fine at two or three windows, worse beyond that; a
/// real task-switcher with titles is the upgrade path.
///
/// These exist because a phone has no keyboard shortcuts. On a desktop compositor every one of
/// them is a key combination.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WindowAction {
    /// Fill the output, or restore to the pre-maximize geometry if already maximized.
    Maximize,
    /// Send to the back of the stack and focus whatever is now on top.
    ///
    /// **Not** an unmap. With no window list, an unmapped window would be unreachable
    /// forever — nothing could name it to bring it back. Lowering keeps it in the cycle, so
    /// "get this out of the way" stays reversible, which is what was actually wanted.
    Minimize,
    /// Ask the window to close (`xdg_toplevel.close`). The app may refuse or prompt.
    Close,
    /// Focus and raise the next window in the stack.
    CycleFocus,
    /// Focus and raise the window with this [`WindowInfo::id`] — the bottom bar's tap target.
    /// An id that no longer exists is a logged no-op: the list the viewer tapped can be a
    /// moment older than the window it names.
    Focus { id: u64 },
    /// Show workspace `n` (1-based).
    Workspace { n: u32 },
    /// Move window `id` to workspace `n` (0: the first empty one), following it if asked.
    MoveToWorkspace { id: u64, n: u32, follow: bool },
    /// The phone's Back button. The compositor picks what "back" means from what is on
    /// screen: Escape when a menu/popup is open or a dialog is in front (dismiss it), otherwise
    /// Alt+Left (the app's own back — browsers, file managers, settings panels). Hiding the
    /// soft keyboard, the first thing Back does, never reaches here: the client does it.
    Back,
    /// Show an empty desktop: every window out of the way. Again, or picking one in the
    /// switcher, brings them back.
    Home,
}

/// One toplevel, as the viewer sees it in the window list.
///
/// State, not events: the compositor publishes the whole list whenever any field changes, so
/// a viewer attaching mid-session is handed the current windows rather than a history.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowInfo {
    /// Stable for the window's lifetime, never reused within a session.
    pub id: u64,
    /// `xdg_toplevel.title`; empty until the app sets one.
    pub title: String,
    /// `xdg_toplevel.app_id`, which by convention is the `.desktop` file's basename. Empty
    /// until the app sets one — the client falls back to a letter tile.
    pub app_id: String,
    /// Holds keyboard focus. At most one window in a list has this set.
    pub focused: bool,
    /// The workspace it is on, 1-based. 0 from a compositor without workspaces.
    #[serde(default)]
    pub workspace: u32,
}

/// The window list and which workspace is showing — what the compositor publishes.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowList {
    pub windows: Vec<WindowInfo>,
    pub workspace: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The browser hand-writes this JSON in `js/control.js` — it does not share these types.
    /// So the wire shape is the actual contract, and these are the exact strings it sends.
    #[test]
    fn accepts_the_json_the_browser_sends() {
        let launch: SessionControl =
            serde_json::from_str(r#"{"Launch":{"command":"weston-terminal"}}"#).unwrap();
        assert!(
            matches!(launch, SessionControl::Launch { command } if command == "weston-terminal")
        );

        for (json, expected) in [
            (r#"{"Window":"maximize"}"#, WindowAction::Maximize),
            (r#"{"Window":"back"}"#, WindowAction::Back),
            (r#"{"Window":"minimize"}"#, WindowAction::Minimize),
            (r#"{"Window":"close"}"#, WindowAction::Close),
            (r#"{"Window":"cycle_focus"}"#, WindowAction::CycleFocus),
        ] {
            let got: SessionControl = serde_json::from_str(json).expect(json);
            assert!(
                matches!(got, SessionControl::Window(a) if a == expected),
                "{json}"
            );
        }
    }

    /// Guards the same names on the relay path, which carries `WindowAction` bare.
    #[test]
    fn window_action_names_are_snake_case() {
        assert_eq!(
            serde_json::to_string(&WindowAction::CycleFocus).unwrap(),
            r#""cycle_focus""#
        );
        assert_eq!(
            serde_json::to_string(&WindowAction::Maximize).unwrap(),
            r#""maximize""#
        );
    }

    /// A verb this build does not know must be rejected, not silently read as another one.
    #[test]
    fn unknown_action_is_rejected() {
        assert!(serde_json::from_str::<SessionControl>(r#"{"Window":"explode"}"#).is_err());
        assert!(serde_json::from_str::<SessionControl>(r#"{"Nope":{}}"#).is_err());
    }
}
