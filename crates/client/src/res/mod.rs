//! Resolutions, and what each one does to the viewer's screen.
//!
//! Four jobs, four files, because they are argued about separately:
//!
//! | | |
//! |---|---|
//! | [`device`] | the sizes that fit *this* screen exactly, derived from its own aspect |
//! | [`catalog`] | every standard mode, for the device this list was not derived from |
//! | [`fit`] | what a given mode does to a given screen — the letterbox arithmetic |
//! | [`scale`] | how large applications draw themselves |
//!
//! The split exists because `fit` is the part that answers the question a user actually asks
//! — *"why can I not tap there?"* — and it has to be callable without the option list.

pub mod catalog;
pub mod device;
pub mod fit;
pub mod scale;

pub use device::{default_value, options};
pub use scale::{SCALES, default_scale};
