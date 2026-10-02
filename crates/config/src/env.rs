//! `WADO_*` environment variables, which still beat the file.
//!
//! They stay because systemd units and the pool script (`scripts/rig.sh`) start N daemons from
//! one config, differing only in env. Precedence is env → config file → built-in default.

use crate::{Config, schema::Turn};

/// Apply every set `WADO_*` variable to `cfg`, returning the names that were applied.
pub fn overlay(cfg: &mut Config) -> Vec<&'static str> {
    overlay_from(cfg, |k| {
        std::env::var(k).ok().filter(|v| !v.trim().is_empty())
    })
}

fn overlay_from(cfg: &mut Config, get: impl Fn(&str) -> Option<String>) -> Vec<&'static str> {
    let mut used = Vec::new();
    let mut take = |name: &'static str| {
        let v = get(name);
        if v.is_some() {
            used.push(name);
        }
        v
    };

    if let Some(v) = take("WADO_RELAY_URL") {
        cfg.server.relay = Some(v);
    }
    if let Some(v) = take("WADO_REMOTE_ID") {
        cfg.server.remote_id = Some(v);
    }
    if let Some(v) = take("WADO_INSTANCE") {
        cfg.server.instance = v.trim().to_string();
    }
    if let Some(n) = take("WADO_UDP_SLICE").and_then(|v| v.trim().parse().ok()) {
        cfg.server.udp_slice = n;
    }
    if let Some(urls) = take("WADO_TURN_URL") {
        let turn = cfg.server.turn.get_or_insert_with(Turn::default);
        turn.url = urls
            .split(',')
            .map(str::trim)
            .filter(|u| !u.is_empty())
            .map(str::to_owned)
            .collect();
    }
    if let Some(v) = take("WADO_TURN_USER") {
        cfg.server.turn.get_or_insert_with(Turn::default).user = v;
    }
    if let Some(v) = take("WADO_TURN_PASS") {
        cfg.server.turn.get_or_insert_with(Turn::default).pass = v;
    }
    if let Some(v) = take("WADO_PUBLIC_RELAY") {
        cfg.server.public_relay = Some(v);
    }
    if let Some(n) = take("WADO_APP_CPU_WEIGHT").and_then(|v| v.trim().parse().ok()) {
        cfg.session.app_cpu_weight = n;
    }
    if let Some(v) = take("WADO_ATSPI") {
        cfg.session.atspi = Some(v.into());
    }
    used
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_beats_the_file_and_says_which() {
        let mut c = crate::kdl::parse("server { relay \"wss://file\"; udp-slice 1 }").unwrap();
        let used = overlay_from(&mut c, |k| match k {
            "WADO_RELAY_URL" => Some("ws://env".into()),
            "WADO_TURN_URL" => Some("turn:a, turn:b".into()),
            _ => None,
        });
        assert_eq!(c.server.relay.as_deref(), Some("ws://env"));
        assert_eq!(c.server.udp_slice, 1, "unset env leaves the file's value");
        assert_eq!(c.server.turn.unwrap().url, ["turn:a", "turn:b"]);
        assert_eq!(used, ["WADO_RELAY_URL", "WADO_TURN_URL"]);
    }
}
