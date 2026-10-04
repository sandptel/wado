//! The end-to-end envelope between a device and this daemon, through the relay
//! (`WADO_PLAN.md`, Decision Log `2026-10-04`; wire in [`wado_protocol::envelope`]).
//!
//! The relay can neither read nor inject a daemon↔device message: every one is sealed with
//! keys only the two ends hold, and the device has proven its long-term key.

pub mod handshake;
pub mod host_key;
pub mod link;
pub mod seal;
