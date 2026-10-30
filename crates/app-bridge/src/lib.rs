//! The Flutter app's bridge to the Rust client core: `api/` is what `flutter_rust_bridge` exposes
//! to Dart (generated into `app/lib/bridge/`). No logic of its own beyond the box key and the
//! event encoding.

pub mod api;
mod exec;
mod frb_generated;
