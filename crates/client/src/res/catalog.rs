//! Every standard mode, for a device whose own aspect is not the only thing worth streaming.
//!
//! The device-exact list in [`super::device`] is the right default and the wrong *only* option:
//! a game that expects 16:9, an application being tested at a desktop shape, or a recording
//! meant to be watched somewhere other than the phone it was made on all want a mode this
//! screen does not have. So the whole list is offered — and every entry says what it will cost
//! here, which is what keeps "all of them" from being a trap.
//!
//! Orientation follows the device: the same panel held the other way wants the same modes
//! transposed, and offering a portrait 1280x720 to a landscape session is offering a mode that
//! is 44% bars.

/// `(long edge, short edge)` of every mode on offer, grouped by the aspect people know it by.
/// Long edge first — orientation is applied per device, not baked in here.
const MODES: [(u32, u32, &str); 22] = [
    // 16:9 — the one everything assumes.
    (1280, 720, "16:9"),
    (1600, 900, "16:9"),
    (1920, 1080, "16:9"),
    (2560, 1440, "16:9"),
    // 16:10 — laptops, and most drawing applications' idea of a canvas.
    (1280, 800, "16:10"),
    (1680, 1050, "16:10"),
    (1920, 1200, "16:10"),
    // 3:2 — Surface, framework laptops, and every photo ever taken on a 35mm frame.
    (1440, 960, "3:2"),
    (2256, 1504, "3:2"),
    // 4:3 — old software, terminals, anything that predates widescreen.
    (1024, 768, "4:3"),
    (1440, 1080, "4:3"),
    (1600, 1200, "4:3"),
    // 5:4 — the shape of a great many industrial and medical applications.
    (1280, 1024, "5:4"),
    // 21:9 and wider — ultrawide desktops.
    (2560, 1080, "21:9"),
    (3440, 1440, "21:9"),
    (3840, 1080, "32:9"),
    // Phone shapes, for a phone being driven from a different device.
    (1600, 720, "20:9"),
    (2400, 1080, "20:9"),
    (1560, 720, "19.5:9"),
    (2340, 1080, "19.5:9"),
    (1440, 720, "18:9"),
    // Square, because a tiling-window screenshot rig is a real thing people build.
    (1080, 1080, "1:1"),
];

/// Every standard mode, in this device's orientation, as `(value, label)` pairs.
///
/// `exclude` drops the ones already offered above as device-exact, so the same size is never
/// in the list twice with two different labels.
pub fn options(sw: u32, sh: u32, exclude: &[String]) -> Vec<(String, String)> {
    let portrait = sh > sw;
    let mut out = Vec::new();
    for (long, short, _) in MODES {
        let (w, h) = if portrait {
            (short, long)
        } else {
            (long, short)
        };
        let value = format!("{w}x{h}");
        if exclude.iter().any(|v| *v == value) {
            continue;
        }
        out.push((value, super::fit::label(sw, sh, w, h)));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modes_follow_the_devices_orientation() {
        let land = options(2400, 1080, &[]);
        assert!(land.iter().any(|(v, _)| v == "1920x1080"), "{land:?}");
        let port = options(1080, 2400, &[]);
        assert!(port.iter().any(|(v, _)| v == "1080x1920"), "{port:?}");
    }

    #[test]
    fn what_fits_is_named_in_the_label() {
        let opts = options(2400, 1080, &[]);
        let (_, l) = opts.iter().find(|(v, _)| v == "2400x1080").unwrap();
        assert!(l.contains("fills this screen"), "{l}");
        let (_, l) = opts.iter().find(|(v, _)| v == "1920x1080").unwrap();
        assert!(l.contains("bars at the sides"), "{l}");
        assert!(l.contains("20%"), "{l}");
    }

    #[test]
    fn the_device_exact_list_is_not_repeated() {
        let opts = options(2400, 1080, &["1600x720".into()]);
        assert!(!opts.iter().any(|(v, _)| v == "1600x720"), "{opts:?}");
    }
}
