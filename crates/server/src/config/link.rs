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
        trackpad: cfg
            .device
            .get(key)
            .and_then(|d| d.trackpad.as_ref())
            .map(Into::into),
        // "May change privileged settings" — the owner, or a device granted `settings`.
        owner: gate.grants(key).settings,
        grants: grants_for(key, gate),
        error: status.error,
        restart: status.restart,
        shells: cfg.shells.enabled,
        gestures: cfg.gestures.clone(),
        binds: cfg.binds.keys.clone(),
        bind_mod: cfg.binds.modifier.clone(),
        window_rules: cfg.window_rule.len(),
    }
}

/// What `key` may do, as the phone shows it. `None` for a device the gate does not know (it is
/// not in yet, so it may do nothing that asks).
fn grants_for(key: &str, gate: &Gate) -> Option<wado_protocol::config::DeviceGrants> {
    let d = gate.device(key)?;
    let g = gate.grants(key);
    Some(wado_protocol::config::DeviceGrants {
        name: Gate::typed(&d.name),
        files: gate.files_access(key).to_string(),
        shells: g.shells,
        settings: g.settings,
        host: g.host,
        input: g.input,
        clipboard: g.clipboard,
        join: g.join,
        takeover: g.takeover,
        paired: d.pinned,
    })
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
    permit(key, gate, cfg_key, confirmed)?;
    wado_config::ui_file::set_values(cfg_key, wado_config::ui_file::parse_values(value))
        .map_err(|e| e.to_string())?;
    tracing::info!(
        device = key,
        key = cfg_key,
        value,
        "config changed from a client"
    );
    super::watch::reload()
}

/// May the device `key` change `cfg_key`? Privileged keys need the settings grant and an
/// on-screen yes — except a device's own trackpad layout (`device.<its key>.trackpad.*`), which
/// is its own business, like its prefs blob.
fn permit(key: &str, gate: &Gate, cfg_key: &str, confirmed: bool) -> Result<(), String> {
    let own = !key.is_empty() && cfg_key.starts_with(&format!("device.{key}.trackpad."));
    if own || tier::of(cfg_key) == Tier::Live {
        return Ok(());
    }
    if !gate.grants(key).settings {
        return Err(format!(
            "{cfg_key} needs the settings grant — on the computer: wado allow <device> settings"
        ));
    }
    if !confirmed {
        return Err(format!("{cfg_key} needs confirming on screen"));
    }
    Ok(())
}

pub fn set_prefs(key: &str, name: &str, prefs: &str) -> Result<(), String> {
    if key.is_empty() {
        return Err("this client has no device key to save settings under".into());
    }
    wado_config::ui_file::set_device(key, name, prefs).map_err(|e| e.to_string())?;
    super::watch::reload()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gate() -> Gate {
        let dir = std::env::temp_dir().join(format!("wado-link-{}", rand::random::<u64>()));
        std::fs::create_dir_all(&dir).unwrap();
        let g = Gate::at(dir);
        g.trust("k0", "laptop");
        g.trust("k1", "phone");
        g.trust("k2", "tab");
        g
    }

    #[test]
    fn a_device_sets_its_own_trackpad_without_the_settings_grant() {
        let g = gate();
        assert!(!g.grants("k1").settings);
        assert_eq!(permit("k1", &g, "device.k1.trackpad.rail", false), Ok(()));
        assert_eq!(
            permit("k1", &g, "device.k1.trackpad.gestures.edge-left", false),
            Ok(())
        );
        let other = permit("k1", &g, "device.k2.trackpad.rail", false).unwrap_err();
        assert!(other.contains("settings grant"), "{other}");
        // A key that only starts like its own is someone else's.
        assert!(permit("k1", &g, "device.k1x.trackpad.rail", false).is_err());
        // The rest of its own block is still the owner's to change.
        assert!(permit("k1", &g, "device.k1.name", false).is_err());
        assert!(permit("", &g, "device..trackpad.rail", false).is_err());
    }

    #[test]
    fn a_device_is_told_its_own_grants() {
        let g = gate();
        g.allow("phone", &["host"], true).unwrap();
        let mine = grants_for("k1", &g).unwrap();
        assert!(mine.host && !mine.input && !mine.settings);
        assert_eq!(mine.name, "phone");
        assert_eq!(mine.files, "none");
        assert_eq!(grants_for("nobody", &g), None);
    }

    #[test]
    fn live_keys_need_no_grant_and_privileged_ones_need_both() {
        let g = gate();
        assert_eq!(permit("k1", &g, "stream.max-fps", false), Ok(()));
        g.allow("phone", &["settings"], true).unwrap();
        assert!(
            permit("k1", &g, "session.autostart", false)
                .unwrap_err()
                .contains("confirming")
        );
        assert_eq!(permit("k1", &g, "session.autostart", true), Ok(()));
    }
}
