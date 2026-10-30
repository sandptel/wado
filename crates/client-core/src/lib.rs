//! wado's client protocol, written once in Rust (docs/superpowers/plans/2026-10-30-native-android.md,
//! Part A §A2). Pure Rust with no FFI: `wado-mobile-core` is its UniFFI façade for the phone app,
//! and the desktop app links it directly. The JS client (`crates/client/src/js`) stays the
//! reference; the shared vectors (`scripts/e2e-vector.mjs`) pin the two together.
//!
//! Re-exports only; each module does one job.

pub mod e2e;
