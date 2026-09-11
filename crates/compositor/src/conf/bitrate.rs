//! Turning a quality preset into a CBR target for the resolution actually being encoded.
//!
//! The presets used to be flat numbers: `Balanced` meant 4000 kbps at 720p and 4000 kbps at
//! 1080p alike. A 1080p frame has 2.25× the pixels of a 720p one, so the same budget bought
//! less than half the bits per pixel, and the rate controller paid for it the only way CBR
//! can — by raising the quantiser. That is visible as blocking the moment the picture moves,
//! which is why it was reported as "pixelates as soon as content is scrolled" and why the
//! *lowest*-bitrate preset was the usable one: its shorter GOP recovered faster.
//!
//! So the preset now names a bits-per-pixel budget, anchored at 1280×720, and the target
//! follows the pixel count.
//!
//! Two deliberate non-obvious choices:
//!
//! **Frame rate is not in the formula.** Doubling the frame rate does not double the bits
//! needed for equal quality — consecutive frames are more alike, so the residual per frame
//! shrinks. Scaling linearly on fps would have asked for 32 Mbps at 1080p120. Leaving it out
//! matches what the code already did and errs toward the safe side.
//!
//! **There is a ceiling, and it is not about the encoder.** webrtc-rs 0.17 has no congestion
//! control on its send path — nothing measures the link or backs off. An uncapped target is
//! therefore a promise the network never agreed to: fine on a LAN, packet loss on cellular.
//! [`CEILING_KBPS`] is the most this will ask for however large the output gets.

/// The resolution the presets are calibrated at. Their numbers are unchanged here.
const BASE_PIXELS: u64 = 1280 * 720;

/// Never ask for more than this, whatever the resolution — see the module note on the
/// missing congestion control.
const CEILING_KBPS: u32 = 12_000;

/// Never ask for less than this; below it even a still picture breaks up.
const FLOOR_KBPS: u32 = 1_000;

/// Scale a preset's 720p budget to `width`×`height`, clamped to a deliverable range.
pub fn for_resolution(base_kbps: u32, width: u32, height: u32) -> u32 {
    let pixels = (width as u64) * (height as u64);
    if pixels == 0 {
        return base_kbps.clamp(FLOOR_KBPS, CEILING_KBPS);
    }
    // u64 throughout: 4K times a five-digit bitrate overflows u32 long before the divide.
    let scaled = (base_kbps as u64 * pixels) / BASE_PIXELS;
    (scaled as u32).clamp(FLOOR_KBPS, CEILING_KBPS)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 2000/4000/8000 are the Reactivity/Balanced/Quality budgets.
    const PRESETS: [u32; 3] = [2000, 4000, 8000];

    #[test]
    fn the_baseline_resolution_is_left_exactly_as_it_was() {
        // The presets were tuned at 720p and are known good there. Scaling must not move
        // them, or this "fix" is a regression for every existing user.
        for base in PRESETS {
            assert_eq!(for_resolution(base, 1280, 720), base);
        }
    }

    #[test]
    fn ten_eighty_p_gets_the_same_bits_per_pixel_as_720p() {
        // 1920x1080 is 2.25x the pixels, so 4000 -> 9000 and the bpp is unchanged. This is
        // the actual bug: Balanced used to hand 1080p the 720p number.
        assert_eq!(for_resolution(4000, 1920, 1080), 9000);
        assert_eq!(for_resolution(2000, 1920, 1080), 4500);
    }

    #[test]
    fn a_device_exact_1080p_scales_too() {
        // The phone-shaped outputs wado actually creates are not 16:9.
        assert_eq!(for_resolution(4000, 1728, 1080), 8100);
        assert_eq!(for_resolution(2000, 1080, 2422), 5676);
    }

    #[test]
    fn the_ceiling_holds_for_the_combination_that_needs_it() {
        // Quality on a tall 1080p phone screen wants ~22 Mbps unclamped. There is no
        // congestion control to discover that the link cannot take it.
        assert_eq!(for_resolution(8000, 1080, 2422), CEILING_KBPS);
        assert_eq!(for_resolution(8000, 3840, 2160), CEILING_KBPS);
    }

    #[test]
    fn a_tiny_output_still_gets_something_usable() {
        assert_eq!(for_resolution(2000, 320, 240), FLOOR_KBPS);
    }

    #[test]
    fn a_degenerate_size_does_not_divide_by_zero_or_panic() {
        assert_eq!(for_resolution(4000, 0, 1080), 4000);
    }

    #[test]
    fn a_large_output_does_not_overflow() {
        // 7680x4320 * 8000 overflows u32 before the divide if this is not done in u64.
        assert_eq!(for_resolution(8000, 7680, 4320), CEILING_KBPS);
    }

    #[test]
    fn scaling_never_inverts_the_preset_order() {
        for (w, h) in [(1280, 720), (1728, 1080), (1920, 1080), (1080, 2422)] {
            let r = for_resolution(2000, w, h);
            let b = for_resolution(4000, w, h);
            let q = for_resolution(8000, w, h);
            assert!(r <= b && b <= q, "{w}x{h}: {r} {b} {q}");
        }
    }
}
