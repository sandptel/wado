//! Settings persistence: one JSON blob, one `localStorage` key, via the bridge.
//!
//! Deliberately not a persistence layer. There is one shape to store, it is stored whole, and
//! [`Saved`] is `#[serde(default)]` throughout — so a blob written by an older build loads
//! with the new fields at their defaults, and a blob with fields this build dropped loads by
//! ignoring them. That is the entire migration story, and it is enough.
//!
//! Debug flags are keyed by `id` rather than by position, because positions change whenever
//! [`crate::debug::ITEMS`] is reordered and a silently scrambled debug panel is a nasty thing
//! to chase.

use std::collections::BTreeMap;

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

use crate::{debug, state::Ui};

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Saved {
    pub conn_mode: Option<String>,
    pub server_addr: Option<String>,
    pub relay_url: Option<String>,
    pub remote_id: Option<String>,

    pub res: Option<String>,
    pub custom_w: Option<u32>,
    pub custom_h: Option<u32>,
    pub scale: Option<String>,
    pub fps: Option<u32>,
    pub fps_lock: Option<bool>,
    pub quality: Option<String>,
    pub bitrate: Option<u32>,
    pub encoder_backend: Option<String>,
    pub placement: Option<String>,
    pub focus_follows: Option<bool>,
    pub repeat_rate: Option<i32>,
    pub repeat_delay: Option<i32>,
    pub preset: Option<String>,
    pub keyframe: Option<String>,
    pub isolate_apps: Option<bool>,
    pub x_server: Option<bool>,

    pub command: Option<String>,
    pub recent: Option<Vec<String>>,
    pub show_hidden: Option<bool>,
    pub move_mode: Option<bool>,
    pub scroll_speed: Option<f64>,
    pub natural_scroll: Option<bool>,

    pub panel_open: Option<bool>,
    pub theme: Option<String>,
    pub theme_custom: Option<String>,

    pub debug_master: Option<bool>,
    /// Debug flags by item id. Unknown ids are ignored on load; missing ones keep their
    /// registry default.
    pub debug: BTreeMap<String, bool>,
}

/// Read every persisted signal. Called from an effect, so reading here is also what
/// subscribes the effect to future changes.
pub fn snapshot(ui: Ui) -> Saved {
    let s = ui.set;
    let flags = s.debug.read().clone();
    let recent = s.recent.read().clone();
    Saved {
        conn_mode: Some((s.conn_mode)()),
        server_addr: Some((s.server_addr)()),
        relay_url: Some((s.relay_url)()),
        remote_id: Some((s.remote_id)()),

        res: Some((s.res)()),
        custom_w: Some((s.custom_w)()),
        custom_h: Some((s.custom_h)()),
        scale: Some((s.scale)()),
        fps: Some((s.fps)()),
        fps_lock: Some((s.fps_lock)()),
        quality: Some((s.quality)()),
        bitrate: Some((s.bitrate)()),
        encoder_backend: Some((s.encoder_backend)()),
        placement: Some((s.placement)()),
        focus_follows: Some((s.focus_follows)()),
        repeat_rate: Some((s.repeat_rate)()),
        repeat_delay: Some((s.repeat_delay)()),
        preset: Some((s.preset)()),
        keyframe: Some((s.keyframe)()),
        isolate_apps: Some((s.isolate_apps)()),
        x_server: Some((s.x_server)()),

        command: Some((s.command)()),
        recent: Some(recent),
        show_hidden: Some((s.show_hidden)()),
        move_mode: Some((s.move_mode)()),
        scroll_speed: Some((s.scroll_speed)()),
        natural_scroll: Some((s.natural_scroll)()),

        panel_open: Some((s.panel_open)()),
        theme: Some((s.theme)()),
        theme_custom: Some((s.theme_custom)()),

        debug_master: Some((s.debug_master)()),
        debug: debug::ITEMS
            .iter()
            .enumerate()
            .map(|(i, item)| {
                (
                    item.id.to_string(),
                    flags.get(i).copied().unwrap_or(item.default),
                )
            })
            .collect(),
    }
}

/// Apply a loaded blob. Every field is optional and a `None` leaves the current default in
/// place, so a partial or older blob is applied as far as it goes rather than rejected.
pub fn restore(ui: Ui, saved: Saved) {
    let mut s = ui.set;
    macro_rules! put {
        ($field:ident) => {
            if let Some(v) = saved.$field {
                s.$field.set(v);
            }
        };
    }
    put!(conn_mode);
    put!(server_addr);
    put!(relay_url);
    put!(remote_id);
    put!(res);
    put!(custom_w);
    put!(custom_h);
    put!(scale);
    put!(fps);
    put!(fps_lock);
    put!(quality);
    put!(bitrate);
    put!(encoder_backend);
    put!(placement);
    put!(focus_follows);
    put!(repeat_rate);
    put!(repeat_delay);
    put!(preset);
    put!(keyframe);
    put!(isolate_apps);
    put!(x_server);
    put!(command);
    put!(recent);
    put!(show_hidden);
    put!(move_mode);
    // Clamped, not just restored: the slider's range shrank from 0.2-5 to 0.05-2, and a
    // blob saved under the old range holds values the control can no longer represent. A
    // range input pins its thumb to the nearest end, so without this the panel would show
    // 2.00x while scrolling at 5x and the user would have no way to reconcile the two.
    if let Some(v) = saved.scroll_speed {
        s.scroll_speed.clone().set(v.clamp(0.05, 2.0));
    }
    put!(natural_scroll);
    put!(panel_open);
    put!(theme);
    put!(theme_custom);
    put!(debug_master);

    if !saved.debug.is_empty() {
        let flags = debug::ITEMS
            .iter()
            .map(|item| saved.debug.get(item.id).copied().unwrap_or(item.default))
            .collect();
        s.debug.set(flags);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_and_missing_fields_both_survive() {
        // A blob from a future build (unknown key) and an older one (missing keys) must both
        // load — this is the whole migration story, so it is the thing worth asserting.
        let blob = r#"{"fps":120,"debug":{"latency":true,"from_the_future":true},
                       "something_we_removed":"x"}"#;
        let saved: Saved = serde_json::from_str(blob).expect("must tolerate both");
        assert_eq!(saved.fps, Some(120));
        assert_eq!(saved.server_addr, None);
        assert_eq!(saved.debug.get("latency"), Some(&true));
    }

    #[test]
    fn debug_flags_are_keyed_by_id_not_position() {
        let saved: Saved = serde_json::from_str(r#"{"debug":{"latency":true}}"#).unwrap();
        // `latency` is not first in the registry, so a positional scheme would have applied
        // this to the wrong toggle.
        assert_ne!(debug::index_of("latency"), 0);
        assert_eq!(saved.debug.get("latency"), Some(&true));
        assert_eq!(saved.debug.get("fps"), None);
    }
}
