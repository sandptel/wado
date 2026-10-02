//! Who may change what from a client.
//!
//! **Live** keys shape the stream and the input; any trusted device may set them.
//! **Privileged** keys run code, widen access or move the daemon: only the owner device may set
//! them, after an on-screen confirmation. The split is by consequence, not by section name —
//! `session.app-cpu-weight` is live while `session.autostart` runs arbitrary commands.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    Live,
    Privileged,
}

const LIVE: &[&str] = &[
    "stream.",
    "input.",
    "session.app-cpu-weight",
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
        assert_eq!(of("session.autostart"), Tier::Privileged);
        assert_eq!(of("session.env.PATH"), Tier::Privileged);
        assert_eq!(of("shells.program"), Tier::Privileged);
        assert_eq!(of("server.relay"), Tier::Privileged);
        assert_eq!(of("streamx.y"), Tier::Privileged);
    }
}
