//! Resolutions that exactly fill *this* device's screen.
//!
//! A stream whose aspect ratio does not match the display is letterboxed, and on a phone the
//! bars are large — a 16:9 stream on a 20:9 screen wastes a third of it. So the options are
//! derived from the device's own aspect rather than offered as a fixed list, and the labels
//! keep the familiar "1080p"/"720p" shorthand, meaning the *short* edge, which is what those
//! names mean on a phone held upright.
//!
//! Isolated from the widgets because this is arithmetic with rules worth testing: the result
//! must stay on the device's aspect and must be even on both axes.

/// Short-edge targets, largest first. 1080 and 720 are the two everyone recognises.
const TARGETS: [u32; 2] = [1080, 720];

/// H.264 4:2:0 subsamples chroma by two, so both axes must be even.
fn even(v: u32) -> u32 {
    v & !1
}

/// Scale `(sw, sh)` so its short edge is `target`, preserving aspect and orientation.
pub fn scaled(sw: u32, sh: u32, target: u32) -> (u32, u32) {
    if sw == 0 || sh == 0 {
        return (target, target);
    }
    let (w, h) = if sw <= sh {
        (target, (sh as f64 * target as f64 / sw as f64).round() as u32)
    } else {
        ((sw as f64 * target as f64 / sh as f64).round() as u32, target)
    };
    (even(w.max(16)), even(h.max(16)))
}

/// The device-exact options, as `(value, label)` pairs ready for a `<select>`.
/// Values are `"WxH"` so they parse with the same rule as every other option.
pub fn options(sw: u32, sh: u32) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for t in TARGETS {
        let (w, h) = scaled(sw, sh, t);
        // Offering an upscale would cost bandwidth for detail the device cannot show.
        if w > sw.max(16) || h > sh.max(16) {
            continue;
        }
        out.push((
            format!("{w}x{h}"),
            format!("{w} × {h} — fills this screen ({t}p)"),
        ));
    }
    out.dedup_by(|a, b| a.0 == b.0);
    out
}

/// What a fresh client should start on: the smaller device-exact option.
///
/// Not the larger one. At 1080×2400/60 the server's WebRTC `write_sample` was measured at
/// 70–103 ms against a 19 ms frame budget, so frames piled up and were dropped before
/// reaching the network — with zero packet loss and 11 ms RTT, i.e. not the link's fault.
/// The smaller option is the one that actually streams.
pub fn default_value(sw: u32, sh: u32) -> Option<String> {
    options(sw, sh).last().map(|(v, _)| v.clone())
}

/// The output scale a device of this pixel density should start at.
///
/// The same reasoning a desktop compositor uses: a display packing three physical pixels
/// into a logical one needs a scale near three, or every app draws at a third of the size it
/// was designed for. Defaulting to 1x on a phone produced a menu twelve physical pixels tall.
///
/// The scales on offer, and the only ones. One list because the default has to be a value the
/// dropdown actually contains — computing a nearby number independently produced 2.75 for a
/// 2.625 display, which matches no option and renders as a blank select.
///
/// Stored as the strings the `<option value>` uses so there is no formatting step to get
/// wrong: `2`, never `2.0`.
pub const SCALES: [(&str, f64, &str); 7] = [
    ("1", 1.0, "1× — native (desktop-sized UI)"),
    ("1.25", 1.25, "1.25×"),
    ("1.5", 1.5, "1.5×"),
    ("1.75", 1.75, "1.75×"),
    ("2", 2.0, "2× — phone-friendly"),
    ("2.5", 2.5, "2.5×"),
    ("3", 3.0, "3×"),
];

/// The offered scale closest to this device's pixel density.
///
/// The same reasoning a desktop compositor uses: a display packing 2.6 physical pixels into a
/// logical one needs a scale near 2.6, or every app draws at a fraction of its design size.
/// Defaulting to 1x on a phone produced a menu twelve physical pixels tall.
///
/// Now that wado speaks `wp-fractional-scale-v1` this no longer rounds to a whole number, so
/// a 2.625 display gets 2.5 rather than being pushed to 3.
pub fn default_scale(dpr: f64) -> &'static str {
    if !dpr.is_finite() || dpr <= 0.0 {
        return SCALES[0].0;
    }
    SCALES
        .iter()
        .min_by(|a, b| {
            (a.1 - dpr).abs().total_cmp(&(b.1 - dpr).abs())
        })
        .map(|(v, _, _)| *v)
        .unwrap_or(SCALES[0].0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_the_device_aspect() {
        // 1080x2400 is 20:9; the 720-class option must be exactly 720x1600.
        assert_eq!(scaled(1080, 2400, 720), (720, 1600));
        assert_eq!(scaled(1080, 2400, 1080), (1080, 2400));
        // Landscape keeps its orientation: the short edge is now the height.
        assert_eq!(scaled(2400, 1080, 720), (1600, 720));
    }

    #[test]
    fn both_axes_stay_even() {
        // 1170x2532 (iPhone-class) scales to a fractional height; it must not land odd.
        for (sw, sh) in [(1170, 2532), (1440, 3088), (828, 1792), (1179, 2556)] {
            for t in TARGETS {
                let (w, h) = scaled(sw, sh, t);
                assert_eq!(w % 2, 0, "{sw}x{sh}@{t} width {w}");
                assert_eq!(h % 2, 0, "{sw}x{sh}@{t} height {h}");
            }
        }
    }

    #[test]
    fn never_offers_an_upscale() {
        // A 720p-class device must not be offered a 1080-class option.
        let opts = options(720, 1600);
        assert_eq!(opts.len(), 1, "{opts:?}");
        assert_eq!(opts[0].0, "720x1600");
    }

    #[test]
    fn default_is_the_smaller_option() {
        assert_eq!(default_value(1080, 2400).as_deref(), Some("720x1600"));
    }

    #[test]
    fn scale_follows_pixel_density() {
        assert_eq!(default_scale(1.0), "1"); // a plain desktop display
        assert_eq!(default_scale(2.0), "2");
        assert_eq!(default_scale(2.625), "2.5"); // this phone — not pushed up to 3
        assert_eq!(default_scale(1.5), "1.5");
        assert_eq!(default_scale(4.0), "3"); // clamped by the list, not extrapolated
    }

    #[test]
    fn absent_or_nonsense_density_falls_back_to_unscaled() {
        for d in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            assert_eq!(default_scale(d), "1", "{d}");
        }
    }

    #[test]
    fn the_default_is_always_an_offered_option() {
        // The bug this replaces: a default computed independently of the list gave 2.75 for
        // a 2.625 display, which matches no <option value> and renders as a blank select.
        for d in [0.75, 1.0, 1.1, 1.5, 2.0, 2.625, 2.9, 3.5, 10.0] {
            let v = default_scale(d);
            assert!(SCALES.iter().any(|(o, _, _)| *o == v), "{d} -> {v:?}");
        }
    }

    #[test]
    fn degenerate_input_does_not_produce_a_zero_sized_output() {
        for (sw, sh) in [(0, 0), (0, 1080), (1080, 0)] {
            let (w, h) = scaled(sw, sh, 720);
            assert!(w >= 16 && h >= 16, "{sw}x{sh} -> {w}x{h}");
        }
    }
}
