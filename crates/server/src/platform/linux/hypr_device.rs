//! One phone's touchpad settings, applied to wado's own touchpad device in Hyprland — runtime
//! only, never a config file, never another device.
//!
//! The pointer speed, scroll speed, natural scrolling, tapping and drag lock a phone chose are
//! libinput's and the compositor's to apply, per device: speed as libinput's speed (scaling the
//! declared pad size instead would scale libinput's millimetre thresholds too, and taps would fail
//! — `docs/superpowers/research/2026-10-30-touchpad-feel.md` §3.2, §4.2). Hyprland keeps them as
//! `device[wado-trackpad]:<key>` and re-reads every pointer's config the moment one is set
//! (`HyprCtl.cpp` `dispatchKeyword`: `COMMAND.contains("device")` → `setPointerConfigs`), and a
//! device that appears later is configured from them too — so the order against the uinput device
//! being created does not matter.
//!
//! **The name.** The uinput device is opened as `wado trackpad` (`touchpad.rs`); Hyprland names a
//! device by lower-casing it and turning spaces, newlines and commas into `-`
//! (`helpers/MiscFunctions.cpp` `deviceNameToInternalString`, 0.56.2), so it is `wado-trackpad`.
//! A second device of the same name would be `wado-trackpad-1` and is not reached.
//!
//! Limits, said out loud: `hyprctl keyword` works with Hyprland's classic config only — with a
//! Lua config it answers "keyword can't work with non-legacy parsers" (logged; `eval` would be the
//! upgrade). A Hyprland config reload drops dynamic keywords; re-applying on its `configreloaded`
//! event is a follow-up. Off Hyprland this does nothing.

use tracing::{debug, info};
use wado_config::schema::DeviceTrackpad;

/// wado's touchpad, as Hyprland names it.
pub const DEVICE: &str = "wado-trackpad";

/// libinput's speed for a speed multiplier `m` (0.5–2×), inverting libinput's touchpad
/// `speed_factor(s) = (s+1)^2.38 · 0.95 + 0.05` — 0.5× → −0.27, 1× → 0, 2× → +0.35.
pub fn sensitivity(m: f64) -> f64 {
    let m = if m.is_finite() {
        m.clamp(0.5, 2.0)
    } else {
        1.0
    };
    (((m - 0.05) / 0.95).powf(1.0 / 2.38) - 1.0).clamp(-1.0, 1.0)
}

/// The keywords that put `t` on wado's device: `(key, value)`, in order. `natural_scroll` only
/// when the phone chose one, so the computer's own setting stands otherwise.
pub fn keywords(t: &DeviceTrackpad) -> Vec<(&'static str, String)> {
    let flag = |b: bool| if b { "1" } else { "0" }.to_string();
    let mut out = vec![
        ("sensitivity", format!("{:.3}", sensitivity(t.speed))),
        (
            "scroll_factor",
            format!("{:.2}", t.scroll_speed.clamp(0.25, 2.0)),
        ),
    ];
    if let Some(n) = t.natural {
        out.push(("natural_scroll", flag(n)));
    }
    out.push(("tap-to-click", flag(t.tap)));
    out.push(("tap-and-drag", flag(t.tap_drag)));
    let lock = match t.drag_lock.as_str() {
        "briefly" => "1",
        "until-tap" => "2",
        _ => "0",
    };
    out.push(("drag_lock", lock.to_string()));
    out
}

/// The `hyprctl --batch` line for `t`: one `keyword device[wado-trackpad]:<key> <value>` each.
pub fn batch(t: &DeviceTrackpad) -> String {
    keywords(t)
        .into_iter()
        .map(|(k, v)| format!("keyword device[{DEVICE}]:{k} {v}"))
        .collect::<Vec<_>>()
        .join(" ; ")
}

/// Is Hyprland running here to apply them?
pub fn available() -> bool {
    super::hyprland::signature().is_some()
}

/// Apply `t` now. Off the hot path: the caller spawns it. Nothing happens off Hyprland.
pub async fn apply(t: DeviceTrackpad) {
    if !available() {
        return;
    }
    match super::hyprland::hyprctl(&["--batch", &batch(&t)]).await {
        Ok(answer)
            if answer
                .lines()
                .all(|l| l.trim().is_empty() || l.trim() == "ok") =>
        {
            debug!("pad: touchpad settings applied to Hyprland");
        }
        Ok(answer) => info!("pad: Hyprland did not take the touchpad settings — {answer}"),
        Err(e) => info!("pad: touchpad settings not applied — {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speed_maps_onto_libinputs_curve() {
        let s = |m: f64| (sensitivity(m) * 100.0).round() / 100.0;
        assert_eq!(s(0.5), -0.27);
        assert_eq!(s(0.75), -0.12);
        assert_eq!(s(1.0), 0.0);
        assert_eq!(s(1.5), 0.19);
        assert_eq!(s(2.0), 0.35);
        assert_eq!(sensitivity(9.0), sensitivity(2.0), "kept to 0.5–2×");
        assert_eq!(sensitivity(0.0), sensitivity(0.5));
        assert_eq!(sensitivity(f64::NAN), 0.0);
    }

    #[test]
    fn the_defaults_leave_natural_scrolling_to_the_computer() {
        assert_eq!(
            batch(&DeviceTrackpad::default()),
            "keyword device[wado-trackpad]:sensitivity 0.000 ; \
             keyword device[wado-trackpad]:scroll_factor 1.00 ; \
             keyword device[wado-trackpad]:tap-to-click 1 ; \
             keyword device[wado-trackpad]:tap-and-drag 1 ; \
             keyword device[wado-trackpad]:drag_lock 0"
        );
    }

    #[test]
    fn every_choice_reaches_its_keyword() {
        let t = DeviceTrackpad {
            speed: 2.0,
            scroll_speed: 0.5,
            natural: Some(false),
            tap: false,
            tap_drag: false,
            drag_lock: "until-tap".into(),
            ..Default::default()
        };
        assert_eq!(
            keywords(&t),
            vec![
                ("sensitivity", "0.353".to_string()),
                ("scroll_factor", "0.50".into()),
                ("natural_scroll", "0".into()),
                ("tap-to-click", "0".into()),
                ("tap-and-drag", "0".into()),
                ("drag_lock", "2".into()),
            ]
        );
        let briefly = DeviceTrackpad {
            drag_lock: "briefly".into(),
            natural: Some(true),
            ..Default::default()
        };
        let k = keywords(&briefly);
        assert!(k.contains(&("drag_lock", "1".into())));
        assert!(k.contains(&("natural_scroll", "1".into())));
    }
}
