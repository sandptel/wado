//! Turning UI state into the wire types the server expects.
//!
//! Isolated from both the widgets and the bridge because it is the one part with rules worth
//! testing: the resolution string and the free-text encoder knobs are parsed here, and a bad
//! parse must land on a sane default rather than a zero-sized output.

use wado_protocol::{
    EncoderBackend, EncoderPref, InputConfig, Placement, Quality, SessionConfig, WindowConfig,
};

use crate::state::Ui;

/// Fallback when the resolution dropdown holds something unparseable. Matches the default
/// option, so a corrupted saved value degrades to the obvious choice instead of 0x0.
const FALLBACK: (u32, u32) = (1280, 720);

/// Parse a `"WIDTHxHEIGHT"` option value. `"custom"` is handled by the caller, which has the
/// custom width/height signals.
fn parse_res(s: &str) -> (u32, u32) {
    let mut it = s.split('x');
    let w = it.next().and_then(|v| v.trim().parse().ok());
    let h = it.next().and_then(|v| v.trim().parse().ok());
    match (w, h) {
        (Some(w), Some(h)) if w > 0 && h > 0 => (w, h),
        _ => FALLBACK,
    }
}

pub fn build(ui: Ui) -> SessionConfig {
    let s = ui.set;
    let res = (s.res)();
    let (width, height) = if res == "custom" {
        ((s.custom_w)(), (s.custom_h)())
    } else {
        parse_res(&res)
    };

    let quality = match (s.quality)().as_str() {
        "reactivity" => Quality::Reactivity,
        "quality" => Quality::Quality,
        "custom" => Quality::Custom {
            bitrate_kbps: (s.bitrate)(),
        },
        _ => Quality::Balanced,
    };
    let placement = match (s.placement)().as_str() {
        "top_left" => Placement::TopLeft,
        "cascade" => Placement::Cascade,
        "maximized" => Placement::Maximized,
        "tile" => Placement::Tile,
        "strip" => Placement::Strip,
        "center" => Placement::Center,
        // "auto": the phone shell on a phone, a desktop everywhere else.
        _ if (ui.live.screen_phone)() => Placement::Strip,
        _ => Placement::Center,
    };
    let backend = match (s.encoder_backend)().as_str() {
        "hardware" => EncoderBackend::Hardware,
        "software" => EncoderBackend::Software,
        _ => EncoderBackend::Auto,
    };

    SessionConfig {
        width,
        height,
        fps: (s.fps)(),
        // A bad parse means unscaled, never zero: a zero scale is a divide-by-zero in the
        // compositor's logical geometry, not a smaller UI.
        scale: (s.scale)()
            .trim()
            .parse()
            .ok()
            .filter(|v: &f32| v.is_finite() && *v > 0.0)
            .unwrap_or(1.0),
        quality,
        preset: Some((s.preset)()).filter(|p| !p.is_empty()),
        // An explicit interval (Advanced) wins. Otherwise "On request" stretches the periodic IDR to
        // a 10 s safety net and leaves the rest to the phone's PLI (invariant #7): a ~22 KB
        // keyframe every second, sent unpaced, was the leading suspect for the jitter that sizes
        // the audio buffer, and video waits for audio (2026-10-03).
        keyframe_interval: (s.keyframe)()
            .trim()
            .parse()
            .ok()
            .or_else(|| ((s.keyframe_mode)() == "on_request").then(|| (s.fps)().max(1) * 10)),
        input: InputConfig {
            repeat_rate: (s.repeat_rate)(),
            repeat_delay: (s.repeat_delay)(),
            focus_follows_pointer: (s.focus_follows)(),
        },
        window: WindowConfig { placement },
        encoder: EncoderPref { backend },
        isolate_apps: (s.isolate_apps)(),
        x_server: (s.x_server)(),
        dark: Some(crate::theme::is_dark(&(s.theme)(), &(s.theme_custom)())),
        background: crate::theme::desktop(&(s.theme)(), &(s.theme_custom)(), &(s.accent)()),
        low_latency_audio: (s.low_latency_audio)(),
        // Only where it can run: a browser without WebCodecs stays on RTP whatever the switch says.
        webcodecs: (s.webcodecs)() && (ui.live.wc_supported)(),
        audio_redundancy: (s.audio_redundancy)(),
        bitrate_cap_kbps: if (s.auto_bitrate)() {
            (ui.live.auto_kbps)()
        } else {
            None
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_dropdown_values() {
        assert_eq!(parse_res("1280x720"), (1280, 720));
        assert_eq!(parse_res("1080x2400"), (1080, 2400));
    }

    #[test]
    fn bad_input_falls_back_rather_than_producing_a_zero_sized_output() {
        for bad in [
            "", "x", "1280", "1280x", "abcxdef", "0x0", "-1x720", "1280x0",
        ] {
            assert_eq!(parse_res(bad), FALLBACK, "{bad:?} should fall back");
        }
    }
}
