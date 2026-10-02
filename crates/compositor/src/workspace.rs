//! Workspaces: numbered desktops, each with its own windows — and, in the sliding layout, its
//! own row of columns.
//!
//! A window belongs to one workspace (`ws_of`). Only the active workspace's windows are mapped;
//! the rest are *parked* — unmapped, their place kept — and come back when their workspace does.
//! In the strip, each workspace keeps its own row: switching stashes the current row and swaps
//! the target's in whole, so every strip routine keeps working on "the row" unchanged.
//!
//! Home is "an empty workspace": the first one with nothing on it. From an empty one it goes
//! back to where you were.

use std::collections::HashMap;

use smithay::{
    desktop::Window,
    utils::{IsAlive, Logical, Point},
};

use crate::Wado;

/// A stashed strip: its columns, focused index and viewport offset.
pub type StripStash = (Vec<crate::strip::Column>, usize, i32);

#[derive(Default)]
pub struct Workspaces {
    /// 1-based.
    pub active: u32,
    /// The workspace before the last switch — what Home goes back to from an empty one.
    pub previous: u32,
    pub of: HashMap<Window, u32>,
    /// Windows of inactive workspaces, unmapped, with where they were.
    pub parked: Vec<(Window, Point<i32, Logical>)>,
    /// The strip rows of inactive workspaces.
    pub strips: HashMap<u32, StripStash>,
}

impl Workspaces {
    pub fn new() -> Self {
        Self {
            active: 1,
            previous: 1,
            ..Default::default()
        }
    }
}

impl Wado {
    /// The workspace a window is on (the active one if it was never filed).
    pub(crate) fn workspace_of(&self, w: &Window) -> u32 {
        self.ws.of.get(w).copied().unwrap_or(self.ws.active)
    }

    /// File a new window on the active workspace.
    pub(crate) fn workspace_adopt(&mut self, w: &Window) {
        self.ws.of.insert(w.clone(), self.ws.active);
    }

    pub(crate) fn is_parked(&self, w: &Window) -> bool {
        self.ws.parked.iter().any(|(p, _)| p == w)
    }

    /// Show workspace `n`.
    pub(crate) fn workspace_switch(&mut self, n: u32) {
        let n = n.max(1);
        if n == self.ws.active {
            return;
        }
        self.prune_workspaces();
        // Park everything showing.
        let shown: Vec<Window> = self.space.elements().cloned().collect();
        for w in shown {
            let loc = self.space.element_location(&w).unwrap_or_default();
            self.space.unmap_elem(&w);
            self.ws.parked.push((w, loc));
        }
        // Swap rows: stash this one, take the target's.
        let row = (
            std::mem::take(&mut self.strip),
            self.strip_focused,
            self.strip_offset,
        );
        self.ws.strips.insert(self.ws.active, row);
        let (strip, focused, offset) = self.ws.strips.remove(&n).unwrap_or_default();
        self.strip = strip;
        self.strip_focused = focused;
        self.strip_offset = offset;
        self.ws.previous = self.ws.active;
        self.ws.active = n;
        // Bring the target's windows back.
        let (back, keep): (Vec<_>, Vec<_>) = std::mem::take(&mut self.ws.parked)
            .into_iter()
            .partition(|(w, _)| self.ws.of.get(w).copied().unwrap_or(1) == n);
        self.ws.parked = keep;
        for (w, loc) in back {
            self.space.map_element(w, loc, false);
        }
        if self.placement == wado_protocol::Placement::Strip {
            self.strip_relayout();
        }
        match self.space.elements().last().cloned() {
            Some(top) => self.focus_window(&top),
            None => {
                let (serial, _) = self.input_clock();
                if let Some(k) = self.seat.get_keyboard() {
                    k.set_focus(self, None, serial);
                }
            }
        }
        tracing::debug!(workspace = n, "workspace");
        self.publish_windows();
    }

    /// Home: the first empty workspace — or, already on an empty one, back to the last.
    pub(crate) fn home(&mut self) {
        self.prune_workspaces();
        let empty_here = !self.ws.of.values().any(|n| *n == self.ws.active);
        if empty_here {
            let back = self.ws.previous;
            if back != self.ws.active {
                self.workspace_switch(back);
            }
            return;
        }
        let used: Vec<u32> = self.ws.of.values().copied().collect();
        let free = (1..).find(|n| !used.contains(n)).unwrap_or(1);
        self.workspace_switch(free);
    }

    /// Move a window to workspace `n` (0: the first empty one), following it there if asked.
    pub(crate) fn workspace_move(&mut self, w: &Window, n: u32, follow: bool) {
        self.prune_workspaces();
        let n = if n == 0 {
            let used: Vec<u32> = self.ws.of.values().copied().collect();
            (1..).find(|k| !used.contains(k)).unwrap_or(1)
        } else {
            n
        };
        let from = self.workspace_of(w);
        if from == n {
            return;
        }
        self.ws.of.insert(w.clone(), n);
        // Out of its row, into the target's.
        let col = if from == self.ws.active {
            let i = self.strip.iter().position(|c| &c.window == w);
            i.map(|i| self.strip.remove(i))
        } else {
            self.ws.strips.get_mut(&from).and_then(|(row, _, _)| {
                row.iter()
                    .position(|c| &c.window == w)
                    .map(|i| row.remove(i))
            })
        };
        if let Some(col) = col {
            if n == self.ws.active {
                self.strip.push(col);
            } else {
                self.ws.strips.entry(n).or_default().0.push(col);
            }
        }
        if from == self.ws.active {
            let loc = self.space.element_location(w).unwrap_or_default();
            self.space.unmap_elem(w);
            self.ws.parked.push((w.clone(), loc));
            self.strip_focused = self.strip_focused.min(self.strip.len().saturating_sub(1));
        } else if n == self.ws.active {
            if let Some(i) = self.ws.parked.iter().position(|(p, _)| p == w) {
                let (w, loc) = self.ws.parked.remove(i);
                self.space.map_element(w, loc, false);
            }
        }
        if self.placement == wado_protocol::Placement::Strip {
            self.strip_relayout();
        }
        if follow {
            self.workspace_switch(n);
            self.focus_window(w);
        }
        self.publish_windows();
    }

    /// Before a window is focused from the switcher: go to its workspace.
    pub(crate) fn workspace_reveal(&mut self, w: &Window) {
        let n = self.workspace_of(w);
        if n != self.ws.active {
            self.workspace_switch(n);
        }
    }

    /// Forget windows that are gone.
    fn prune_workspaces(&mut self) {
        self.ws.of.retain(|w, _| w.alive());
        self.ws.parked.retain(|(w, _)| w.alive());
        for (row, focused, _) in self.ws.strips.values_mut() {
            row.retain(|c| c.window.alive());
            *focused = (*focused).min(row.len().saturating_sub(1));
        }
    }

    /// Every window on an inactive workspace, in its row's order, for the window list.
    pub(crate) fn parked_windows(&self) -> Vec<Window> {
        self.ws
            .parked
            .iter()
            .map(|(w, _)| w.clone())
            .filter(|w| w.alive())
            .collect()
    }

    /// A window's column within its own workspace's row, if it has one.
    pub(crate) fn column_of(&self, w: &Window) -> Option<usize> {
        if self.workspace_of(w) == self.ws.active {
            return self.strip.iter().position(|c| &c.window == w);
        }
        self.ws
            .strips
            .get(&self.workspace_of(w))
            .and_then(|(row, _, _)| row.iter().position(|c| &c.window == w))
    }
}
