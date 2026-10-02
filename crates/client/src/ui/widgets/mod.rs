//! Small, reused building blocks for the control centre. Each file is one control.

mod badge;
mod icon;
mod info;
mod navrow;
mod seg;
mod switch;

pub use badge::{When, WhenBadge};
pub use icon::Icon;
pub use info::Info;
pub use navrow::NavRow;
pub use seg::Seg;
pub use switch::SwitchRow;
