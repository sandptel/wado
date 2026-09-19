//! Run lanes — which subsystem this session is investigating.
//!
//! A run stays in one lane (see `plan/RUNS.md`), and each lane needs a different part of the
//! tree loud and the rest quiet. `WADO_RUN=<lane>` picks one; `RUST_LOG` still overrides
//! everything, as it always did.
//!
//! A tracing target *is* the module path, so a lane is nothing but an `EnvFilter` string and
//! this module is nothing but the table of them. Deliberately not a Cargo feature: lanes are
//! switched several times an hour mid-session, and a feature would cost a full fat-LTO release
//! rebuild — of the one binary that must never be a debug build — to change a log level.
//!
//! ponytail: a `&str` table, no lane type, no registry. Add a lane by adding a row.

/// Quiet in every lane: eight "Failed to close candidate … agent is closed" lines per teardown,
/// none of which has ever meant anything. The connection lane raises it on purpose.
const ICE_NOISE: &str = "webrtc_ice=warn";

/// `(lane, what it makes loud)`. The first row is the default.
///
/// Each string is appended to a common `info,wado=debug,wado_compositor=debug` base, so a lane
/// only ever names what it wants *extra* — and a module nobody named still reaches `debug` if it
/// belongs to wado.
const LANES: &[(&str, &str, &str)] = &[
    // 1. Performance — where frames and milliseconds go.
    (
        "perf",
        "fps anomalies, pacing, congestion, the frame pump",
        "wado::pumpstats=trace,wado_compositor::pacing=trace,wado_compositor::congestion=trace,\
         wado_compositor::encode=debug,wado_compositor::capture=debug",
    ),
    // 2. Connection hardening — why a device failed to connect, and which device it was.
    (
        "connection",
        "signalling, ICE, the pool, per-device attribution",
        "wado::relay_client=trace,wado::ice=trace,wado::webrtc_settings=debug,\
         webrtc_ice=info,webrtc::peer_connection=debug,wado_relay=debug",
    ),
    // 3. Features — app launching, the PTY, the control surface a feature is plumbed through.
    (
        "feature",
        "app launcher, PTY, protocol handlers, the control plane",
        "wado::apps=trace,wado::pty=trace,wado::website=debug,\
         wado_compositor::proc=trace,wado_compositor::handlers=debug",
    ),
    // 4. Compositor — windows, surfaces, protocols, input synthesis. NOT the per-frame path:
    //    `wado_compositor=trace` would bury all of this under the render loop.
    (
        "compositor",
        "windows, surfaces, wayland protocols, input synthesis",
        "wado_compositor::handlers=trace,wado_compositor::window=trace,\
         wado_compositor::placement=trace,wado_compositor::state=trace,\
         wado_compositor::input=trace,wado_compositor::proc=trace",
    ),
];

const BASE: &str = "info,wado=debug,wado_compositor=debug";

/// The lane this process is running in, and the filter it implies.
///
/// Returns `(lane name, what it makes loud, filter)`. An unknown name falls back to the default
/// lane rather than failing: a typo in an env var must not stop the daemon coming up, but it
/// must not silently look like the lane you asked for either — hence the name is logged.
pub fn resolve() -> (&'static str, &'static str, String) {
    let wanted = std::env::var("WADO_RUN").unwrap_or_default();
    let (name, what, extra) = LANES
        .iter()
        .find(|(n, _, _)| *n == wanted.trim())
        .copied()
        .unwrap_or(LANES[0]);
    let ice = if name == "connection" { "" } else { ICE_NOISE };
    let filter = [BASE, extra, ice].iter().filter(|s| !s.is_empty()).cloned().collect::<Vec<_>>().join(",");
    (name, what, filter)
}

/// Every lane name, for an error message.
pub fn names() -> String {
    LANES.iter().map(|(n, _, _)| *n).collect::<Vec<_>>().join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The filter strings are hand-written and a typo in one is invisible until the lane is used
    /// — at which point the daemon logs nothing useful and looks broken. Parse them all.
    #[test]
    fn every_lane_parses_as_a_filter() {
        for (name, _, extra) in LANES {
            let f = [BASE, extra, ICE_NOISE].join(",");
            tracing_subscriber::EnvFilter::try_new(&f)
                .unwrap_or_else(|e| panic!("lane {name} has an unparseable filter: {e}\n{f}"));
        }
    }

    #[test]
    fn an_unknown_lane_falls_back_to_the_first() {
        unsafe { std::env::set_var("WADO_RUN", "nonsense") };
        assert_eq!(resolve().0, LANES[0].0);
        unsafe { std::env::remove_var("WADO_RUN") };
    }
}
