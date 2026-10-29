//! The pages about one computer, each built from the sections in `ui/pages/`.
//!
//! - [`display_session`] — its session's shape and tuning, and what it holds for every device.
//! - [`sound_devices`] — its mixer, outputs and Bluetooth.
//! - [`connection_trust`] — how it is reached, what this phone may do there, its Wi-Fi, forget it.

pub mod connection_trust;
pub mod display_session;
pub mod sound_devices;
