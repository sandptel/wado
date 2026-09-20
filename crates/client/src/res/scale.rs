//! How large applications draw themselves inside the session.
//!
//! Its own file because it is a different question from *how many pixels the stream carries*:
//! the scale changes the logical area applications lay out in, and nothing about the encode.

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
        .min_by(|a, b| (a.1 - dpr).abs().total_cmp(&(b.1 - dpr).abs()))
        .map(|(v, _, _)| *v)
        .unwrap_or(SCALES[0].0)
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
