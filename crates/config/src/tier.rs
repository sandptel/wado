//! Who may change what from a client.
//!
//! **Live** keys shape the stream and the input; any trusted device may set them.
//! **Privileged** keys run code, widen access or move the daemon: only the owner device may set
//! them, after an on-screen confirmation. The split is by consequence, not by section name —
//! `session.app-cpu-weight` is live while `session.autostart` runs arbitrary commands.
//!
//! `device.*` is privileged here, with one exception that needs the requester's key and so lives
//! in `wado_server::config::link`: a device may set its own `device.<its key>.trackpad.*`.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    Live,
    Privileged,
}

const LIVE: &[&str] = &[
    "stream.",
    "input.",
    "trackpad.px-per-mm",
    "trackpad.smoothing",
    "trackpad.rate-hz",
    "trackpad.delay",
    "trackpad.direct-delay-ms",
    "trackpad.direct-delay-min",
    "trackpad.direct-delay-max",
    "session.app-cpu-weight",
    "session.audio",
    "binds.",
    "gestures.",
    "window-rule",
];

pub fn of(key: &str) -> Tier {
    if LIVE
        .iter()
        .any(|p| key.starts_with(p) || key == p.trim_end_matches('.'))
    {
        Tier::Live
    } else {
        Tier::Privileged
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_running_keys_are_privileged() {
        assert_eq!(of("stream.max-fps"), Tier::Live);
        assert_eq!(of("input.repeat-rate"), Tier::Live);
        assert_eq!(of("session.app-cpu-weight"), Tier::Live);
        assert_eq!(of("trackpad.smoothing"), Tier::Live);
        assert_eq!(of("trackpad.rate-hz"), Tier::Live);
        assert_eq!(of("trackpad.direct-delay-ms"), Tier::Live);
        assert_eq!(of("trackpad.delay"), Tier::Live);
        assert_eq!(of("trackpad.direct-delay-min"), Tier::Live);
        assert_eq!(of("trackpad.direct-delay-max"), Tier::Live);
        assert_eq!(of("trackpad.enabled"), Tier::Privileged);
        assert_eq!(of("session.autostart"), Tier::Privileged);
        assert_eq!(of("session.env.PATH"), Tier::Privileged);
        assert_eq!(of("shells.program"), Tier::Privileged);
        assert_eq!(of("server.relay"), Tier::Privileged);
        assert_eq!(of("streamx.y"), Tier::Privileged);
    }
}
