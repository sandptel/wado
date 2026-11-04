//! The shared experience model (`wado_ux`), for the app: one stable call. A new shared rule is an
//! arm in `wado_ux::op` — called by name from Dart (`app/lib/wado/ux.dart`) with no new binding.

/// `wado_ux::op::call`: `op` and its JSON input → JSON. Sync: it is pure and microseconds long,
/// and the app reads it while building a frame.
#[flutter_rust_bridge::frb(sync)]
pub fn ux(op: String, input: String) -> String {
    wado_ux::op::call(&op, &input)
}
