//! One computer's link on a seatless control room: its state machine, reconnect delays, the
//! client side of the lane, and the task that runs it.

pub mod backoff;
pub mod driver;
pub mod outbox;
pub mod state;
