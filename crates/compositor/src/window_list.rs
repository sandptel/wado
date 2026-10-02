//! The window list published to the viewer.
//!
//! One job: give every toplevel a stable id and publish `(id, title, app_id, focused)` whenever
//! any of it changes. The viewer draws its bottom bar from this and names windows back by id
//! (`WindowAction::Focus`), which is what lifts window actions off "the focused window only".
//!
//! Published from the post-dispatch hook rather than from each place a window can change —
//! title and app_id land on commit, focus moves from half a dozen call sites, windows map and
//! die on their own schedule. Rebuilding a list of a handful of strings once per loop iteration
//! and sending only on a difference is cheaper to keep correct than instrumenting all of them.
//!
//! ponytail: rebuilt per dispatch, O(windows) with a lock per surface. Fine at phone-scale
//! window counts; move to change hooks if a profile ever shows it.

use smithay::desktop::Window;
use smithay::utils::IsAlive;
use smithay::wayland::compositor::with_states;
use smithay::wayland::shell::xdg::XdgToplevelSurfaceData;
use wado_protocol::WindowInfo;

use crate::Wado;

impl Wado {
    /// Publish the window list if it changed since the last call.
    pub fn publish_windows(&mut self) {
        self.window_ids.retain(|w, _| w.alive());
        let focused = self.focused_window();
        // Every workspace's windows (see `crate::workspace`): the switcher is how they come back.
        let windows: Vec<Window> = self
            .space
            .elements()
            .cloned()
            .chain(self.parked_windows())
            .collect();
        let mut list: Vec<WindowInfo> = windows
            .iter()
            .filter_map(|w| {
                let surface = w.toplevel()?.wl_surface().clone();
                let (title, app_id) = with_states(&surface, |states| {
                    let attrs = states
                        .data_map
                        .get::<XdgToplevelSurfaceData>()?
                        .lock()
                        .ok()?;
                    Some((attrs.title.clone(), attrs.app_id.clone()))
                })?;
                Some(WindowInfo {
                    workspace: self.workspace_of(w),
                    id: self.window_id(w),
                    title: title.unwrap_or_default(),
                    app_id: app_id.unwrap_or_default(),
                    focused: focused.as_ref() == Some(w),
                })
            })
            .collect();
        // Strip order when there is a strip (its dialogs after), map order otherwise — ids
        // follow map order. Never `space.elements()` order: that is stacking, every raise
        // reshuffles it, and a bar whose icons jump on each tap is unusable.
        // Within a workspace, its row's order.
        let column = |id: u64| {
            self.window_by_id(id)
                .and_then(|w| self.column_of(&w))
                .unwrap_or(usize::MAX)
        };
        list.sort_by_key(|w| (w.workspace, column(w.id), w.id));
        // In the strip the list *is* the row: the dial's slot i is column i, so dialogs (which
        // float over their parent) stay out of it.
        if self.placement == wado_protocol::Placement::Strip {
            list.retain(|w| column(w.id) != usize::MAX);
        }
        let list = wado_protocol::WindowList {
            windows: list,
            workspace: self.ws.active,
        };
        self.windows_tx.send_if_modified(|cur| {
            if *cur == list {
                return false;
            }
            *cur = list;
            true
        });
    }

    /// The window's id, assigning the next one on first sight.
    fn window_id(&mut self, window: &Window) -> u64 {
        if let Some(id) = self.window_ids.get(window) {
            return *id;
        }
        self.next_window_id += 1;
        self.window_ids.insert(window.clone(), self.next_window_id);
        self.next_window_id
    }

    /// The live window an id names, if it still exists.
    pub(crate) fn window_by_id(&self, id: u64) -> Option<Window> {
        self.window_ids
            .iter()
            .find(|(w, i)| **i == id && w.alive())
            .map(|(w, _)| w.clone())
    }
}
