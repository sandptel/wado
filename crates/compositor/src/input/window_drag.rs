//! Compositor-managed window move: [`wado_protocol::InputEvent::WindowDrag`] repositions the
//! window under the drag **without involving the app** (touch long-press-drag, or the
//! client's "Move mode"). A plain state machine over `Wado::window_move`, distinct from the
//! app-initiated CSD grabs in `grabs/` — so it never fights Wayland touch/pointer routing.

use wado_protocol::TouchPhase;

use crate::{Wado, state::WindowMove};

impl Wado {
    /// Drive the interactive window move for one `WindowDrag` event.
    pub(crate) fn window_drag(&mut self, phase: TouchPhase, x: f64, y: f64) {
        let Some(loc) = self.map_point(x, y) else {
            return;
        };
        match phase {
            TouchPhase::Down => {
                let (serial, _) = self.input_clock();
                if let Some((window, _)) = self.space.element_under(loc).map(|(w, l)| (w.clone(), l)) {
                    self.space.raise_element(&window, true);
                    self.focus_window_at(loc, serial);
                    let start_win =
                        self.space.element_location(&window).unwrap_or_else(|| (0, 0).into());
                    self.window_move = Some(WindowMove { window, start_ptr: loc, start_win });
                }
            }
            TouchPhase::Motion => {
                if let Some(mv) = &self.window_move {
                    let delta = loc - mv.start_ptr;
                    let new_loc = (mv.start_win.to_f64() + delta).to_i32_round();
                    let window = mv.window.clone();
                    self.space.map_element(window, new_loc, true);
                }
            }
            TouchPhase::Up => {
                self.window_move = None;
            }
        }
    }
}
