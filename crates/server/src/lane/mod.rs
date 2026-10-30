//! The daemon's side of the fallback lane (Decision Log `2026-10-27`): everything it sends over
//! the relay socket is held a little under the relay's 64 KB/s-per-seat cap, so a relay that
//! enforces the cap (`lane`) never has to queue, and one from before it is not flooded either.
//! Signalling never waits behind a transfer: urgent frames jump the queue before they are
//! sealed, so the envelope's counter order is untouched.
//!
//! | file | job |
//! |---|---|
//! | [`classify`] | which frames are urgent, bulk, spare, or not the lane's |
//! | [`outbox`] | one link's queue: order and pace |
//! | [`gauge`] | how much bulk is queued — what a streaming producer waits on |
//! | [`pump`] | the loop from a link's channel to its socket |

pub mod classify;
pub mod gauge;
pub mod outbox;
pub mod pump;

pub use gauge::Gauge;
pub use outbox::Outbox;
