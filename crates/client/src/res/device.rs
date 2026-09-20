//! Resolutions that exactly fill *this* device's screen.
//!
//! A stream whose aspect ratio does not match the display is letterboxed, and on a phone the
//! bars are large — a 16:9 stream on a 20:9 screen wastes a fifth of it. So these options are
//! derived from the device's own aspect rather than picked off a list, and the labels keep the
//! familiar "1080p"/"720p" shorthand, meaning the *short* edge, which is what those names mean
//! on a phone held upright.
//!
//! Isolated from the widgets because this is arithmetic with rules worth testing: the result
//! must stay on the device's aspect and must be even on both axes.

/// Short-edge targets, largest first. Several rungs rather than two, because "fills the screen"
/// and "streams well on this link" are different questions and the user is entitled to both.
const TARGETS: [u32; 5] = [1440, 1080, 900, 720, 540];

/// The rung a fresh client starts on. Not the largest: at 1080×2400/60 the server's WebRTC
/// `write_sample` was measured at 70–103 ms against a 19 ms frame budget, so frames piled up
/// and were dropped before reaching the network — with zero packet loss and 11 ms RTT, i.e.
/// not the link's fault. 720 is the rung that actually streams.
const DEFAULT_TARGET: u32 = 720;

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
        (
            target,
            (sh as f64 * target as f64 / sw as f64).round() as u32,
        )
    } else {
        (
            (sw as f64 * target as f64 / sh as f64).round() as u32,
            target,
        )
    };
    (even(w.max(16)), even(h.max(16)))
}

/// The device-exact options, as `(value, label)` pairs ready for a `<select>`.
/// Values are `"WxH"` so they parse with the same rule as every other option.
pub fn options(sw: u32, sh: u32, phone: bool) -> Vec<(String, String)> {
    let device = if phone { "your phone" } else { "your screen" };
    let mut out: Vec<(String, String)> = Vec::new();
    for t in TARGETS {
        let (w, h) = scaled(sw, sh, t);
        // Offering an upscale would cost bandwidth for detail the device cannot show.
        if w > sw.max(16) || h > sh.max(16) {
            continue;
        }
        let value = format!("{w}x{h}");
        if out.iter().any(|(v, _)| *v == value) {
            continue;
        }
        out.push((
            value,
            format!(
                "{w} × {h} — {} {} · {device} · fills it ({t}p)",
                super::fit::aspect(w, h),
                super::fit::orientation(w, h),
            ),
        ));
    }
    // A screen shorter than the smallest rung — a 640x480 panel, a small embedded display —
    // would otherwise be offered nothing at all, and an empty group is how a blank `<select>`
    // happens. Its own pixels always fit.
    if out.is_empty() {
        let (w, h) = (even(sw.max(16)), even(sh.max(16)));
        out.push((
            format!("{w}x{h}"),
            format!("{w} × {h} — fills this screen (native)"),
        ));
    }
    out
}

/// What a fresh client should start on: the 720-class rung when this screen has one, and
/// otherwise the largest it does have — never a rung that is not in the list, which is how a
/// blank `<select>` used to happen.
pub fn default_value(sw: u32, sh: u32) -> Option<String> {
    let opts = options(sw, sh, false);
    let (w, h) = scaled(sw, sh, DEFAULT_TARGET);
    let exact = format!("{w}x{h}");
    opts.iter()
        .find(|(v, _)| *v == exact)
        .or_else(|| opts.first())
        .map(|(v, _)| v.clone())
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
        // A 720p-class device gets the rungs at or below its own panel and no more.
        let opts = options(720, 1600, true);
        assert!(
            opts.iter().all(|(v, _)| v == "720x1600" || v == "540x1200"),
            "{opts:?}"
        );
        assert_eq!(opts.len(), 2, "{opts:?}");
    }

    #[test]
    fn every_device_option_actually_fills_the_screen() {
        for (sw, sh) in [(1080u32, 2400u32), (1179, 2556), (2400, 1080), (1920, 1080)] {
            for (v, _) in options(sw, sh, false) {
                let (w, h) = v.split_once('x').unwrap();
                let (w, h) = (w.parse().unwrap(), h.parse().unwrap());
                assert_eq!(
                    super::super::fit::of(sw, sh, w, h).0,
                    super::super::fit::Fit::Fills,
                    "{sw}x{sh} offered {v}"
                );
            }
        }
    }

    #[test]
    fn the_default_is_the_720_rung_and_is_always_on_offer() {
        assert_eq!(default_value(1080, 2400).as_deref(), Some("720x1600"));
        // Including screens below the smallest rung: they fall back to their own pixels
        // rather than to an empty list.
        for (sw, sh) in [(1080u32, 2400u32), (640, 480), (3840, 2160), (400, 900)] {
            let d = default_value(sw, sh).unwrap();
            assert!(
                options(sw, sh, false).iter().any(|(v, _)| *v == d),
                "{sw}x{sh} -> {d}"
            );
        }
        assert_eq!(default_value(640, 480).as_deref(), Some("640x480"));
    }

    #[test]
    fn degenerate_input_does_not_produce_a_zero_sized_output() {
        for (sw, sh) in [(0, 0), (0, 1080), (1080, 0)] {
            let (w, h) = scaled(sw, sh, 720);
            assert!(w >= 16 && h >= 16, "{sw}x{sh} -> {w}x{h}");
        }
    }
}
