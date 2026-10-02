//! What a viewer is told about the config, and what it may change.

use wado_config::tier::{self, Tier};
use wado_protocol::{ConfigState, HostLimits, PinnedInput};

use crate::gate::Gate;

/// The config as the device `key` sees it.
pub fn state_for(key: &str, gate: &Gate) -> ConfigState {
    let cfg = wado_config::live::current();
    let status = super::current_status();
    let s = &cfg.stream;
    ConfigState {
        limits: HostLimits {
            max_fps: s.max_fps,
            max_bitrate: s.max_bitrate,
            max_width: s.max_width,
            max_height: s.max_height,
            encoder: s.encoder.forced(),
        },
        input: PinnedInput {
            repeat_rate: cfg.input.repeat_rate,
            repeat_delay: cfg.input.repeat_delay,
            focus_follows_pointer: cfg.input.focus_follows_pointer,
        },
        prefs: cfg.device.get(key).and_then(|d| d.prefs.clone()),
        owner: is_owner(key, gate),
        error: status.error,
        restart: status.restart,
        shells: cfg.shells.enabled,
        gestures: cfg.gestures.clone(),
        binds: cfg.binds.keys.clone(),
        bind_mod: cfg.binds.modifier.clone(),
        window_rules: cfg.window_rule.len(),
    }
}

/// The owner is named in `security { owner }`, else it is the first device ever trusted.
pub fn is_owner(key: &str, gate: &Gate) -> bool {
    if key.is_empty() {
        return false;
    }
    match &wado_config::live::current().security.owner {
        Some(owner) => owner == key,
        None => gate.first_trusted().as_deref() == Some(key),
    }
}

/// Apply a viewer's `ConfigSet`, then reload so it takes effect now rather than on the next
/// poll.
pub fn set(
    key: &str,
    gate: &Gate,
    cfg_key: &str,
    value: &str,
    confirmed: bool,
) -> Result<(), String> {
    if tier::of(cfg_key) == Tier::Privileged {
        if !is_owner(key, gate) {
            return Err(format!(
                "{cfg_key} can only be changed from the owner device"
            ));
        }
        if !confirmed {
            return Err(format!("{cfg_key} needs confirming on screen"));
        }
    }
    wado_config::ui_file::set(cfg_key, wado_config::ui_file::parse_value(value))
        .map_err(|e| e.to_string())?;
    tracing::info!(
        device = key,
        key = cfg_key,
        value,
        "config changed from a client"
    );
    super::watch::reload()
}

pub fn set_prefs(key: &str, name: &str, prefs: &str) -> Result<(), String> {
    if key.is_empty() {
        return Err("this client has no device key to save settings under".into());
    }
    wado_config::ui_file::set_device(key, name, prefs).map_err(|e| e.to_string())?;
    super::watch::reload()
}
