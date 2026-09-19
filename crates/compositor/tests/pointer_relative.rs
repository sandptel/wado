//! Relative pointer motion has to actually move the pointer.
//!
//! An integration test rather than a unit one because the thing worth asserting is the whole
//! path: a `wado_protocol::InputEvent` off the wire, through the dispatcher, to the seat's own
//! idea of where the pointer is. The arithmetic is unit-tested next to `clamp_point`; what this
//! catches is the wiring — a missing match arm, or deltas being treated as a position.

use smithay::utils::{Logical, Point};
use wado_protocol::InputEvent;

fn location(state: &wado_compositor::Wado) -> Point<f64, Logical> {
    state
        .seat
        .get_pointer()
        .expect("seat has a pointer")
        .current_location()
}

#[test]
fn deltas_accumulate_into_the_pointer_position() {
    let (frame_tx, _frame_rx) = tokio::sync::mpsc::channel(2);
    let (_event_loop, mut state, _handles) = wado_compositor::build(frame_tx).expect("build");

    // No render pipeline: this is input synthesis, and standing up a GPU session would prove
    // nothing extra. There is no output either, so nothing clamps — which is the point here.
    state.session_active = true;
    let start = location(&state);

    state.handle_remote_input(InputEvent::PointerRelative { dx: 10.0, dy: 4.0 });
    state.handle_remote_input(InputEvent::PointerRelative { dx: -3.0, dy: 6.0 });

    let moved = location(&state) - start;
    assert_eq!(moved, (7.0, 10.0).into(), "deltas must sum, not replace");
}

#[test]
fn nothing_moves_without_a_session() {
    let (frame_tx, _frame_rx) = tokio::sync::mpsc::channel(2);
    let (_event_loop, mut state, _handles) = wado_compositor::build(frame_tx).expect("build");

    let start = location(&state);
    state.handle_remote_input(InputEvent::PointerRelative { dx: 50.0, dy: 50.0 });
    assert_eq!(
        location(&state),
        start,
        "input before Start must be dropped"
    );
}
