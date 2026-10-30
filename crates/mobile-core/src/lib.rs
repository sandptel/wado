//! wado's shared core for the native phone apps (android/, docs/android.md), exported over
//! UniFFI. The rule for what lives here: anything the web client and a native app must agree on
//! byte for byte — wire messages, their pacing, the pairing check words — is written once, in
//! Rust, against `wado_protocol`'s own types, rather than re-typed in Kotlin.
//!
//! - [`pad`]: the trackpad's touch frames and when they go out (the JS original is
//!   crates/client/src/js/pad_touch.js).
//! - [`words`]: a typed pairing's four check words.

pub mod pad;
pub mod words;

uniffi::setup_scaffolding!();

/// This core's version, to show beside the app's.
#[uniffi::export]
pub fn core_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}
