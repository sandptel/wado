//! Home: an empty desktop. Every window is put out of the way — unmapped, its place kept — and
//! keyboard focus is dropped, so a launch from the drawer opens onto a clear screen.
//!
//! Home again brings them all back; picking one in the switcher brings that one back. The
//! window list keeps listing hidden windows, which is what lets the switcher reach them.

use smithay::{
    desktop::Window,
    utils::{IsAlive, Logical, Point},
};

use crate::Wado;

impl Wado {
    pub(crate) fn home(&mut self) {
        let shown: Vec<Window> = self.space.elements().cloned().collect();
        if shown.is_empty() {
            // Nothing out: Home is "bring them back".
            for (w, loc) in std::mem::take(&mut self.hidden) {
                if w.alive() {
                    self.space.map_element(w, loc, false);
                }
            }
            if let Some(top) = self.space.elements().last().cloned() {
                self.focus_window(&top);
            }
        } else {
            for w in shown {
                let loc = self.space.element_location(&w).unwrap_or_default();
                self.space.unmap_elem(&w);
                self.hidden.push((w, loc));
            }
            let (serial, _) = self.input_clock();
            if let Some(keyboard) = self.seat.get_keyboard() {
                keyboard.set_focus(self, None, serial);
            }
        }
        self.publish_windows();
    }

    /// Bring one hidden window back, before it is focused.
    pub(crate) fn unhide(&mut self, window: &Window) {
        if let Some(i) = self.hidden.iter().position(|(w, _)| w == window) {
            let (w, loc): (Window, Point<i32, Logical>) = self.hidden.remove(i);
            self.space.map_element(w, loc, false);
        }
    }

    pub(crate) fn is_hidden(&self, window: &Window) -> bool {
        self.hidden.iter().any(|(w, _)| w == window)
    }
}
