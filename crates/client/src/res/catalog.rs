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
//!
//! Each mode names **what kind of screen it is** — "desktop & TV", "phone", "tablet" — because
//! a bare number is not a suggestion. That class is declared per row rather than inferred from
//! the ratio: 21:9 (2.33) and 20:9 (2.22) are a hair apart numerically and are an ultrawide
//! monitor and a phone respectively, which no threshold separates honestly.

/// `(long edge, short edge, aspect, what kind of screen that is)` for every mode on offer.
/// Long edge first — orientation is applied per device, not baked in here.
const MODES: [(u32, u32, &str, &str); 22] = [
    // 16:9 — the one everything assumes.
    (1280, 720, "16:9", "desktop & TV"),
    (1600, 900, "16:9", "desktop & TV"),
    (1920, 1080, "16:9", "desktop & TV"),
    (2560, 1440, "16:9", "desktop & TV"),
    // 16:10 — laptops, and most drawing applications' idea of a canvas.
    (1280, 800, "16:10", "laptop"),
    (1680, 1050, "16:10", "laptop"),
    (1920, 1200, "16:10", "laptop"),
    // 3:2 — Surface, Framework laptops, and every 35mm photo ever taken.
    (1440, 960, "3:2", "laptop & tablet"),
    (2256, 1504, "3:2", "laptop & tablet"),
    // 4:3 — old software, terminals, anything that predates widescreen.
    (1024, 768, "4:3", "tablet & older desktop"),
    (1440, 1080, "4:3", "tablet & older desktop"),
    (1600, 1200, "4:3", "tablet & older desktop"),
    // 5:4 — the shape of a great many industrial and medical applications.
    (1280, 1024, "5:4", "older desktop"),
    // 21:9 and wider — ultrawide desktops.
    (2560, 1080, "21:9", "ultrawide desktop"),
    (3440, 1440, "21:9", "ultrawide desktop"),
    (3840, 1080, "32:9", "superwide desktop"),
    // Phone shapes, for a phone being driven from a different device.
    (1600, 720, "20:9", "phone"),
    (2400, 1080, "20:9", "phone"),
    (1560, 720, "19.5:9", "phone"),
    (2340, 1080, "19.5:9", "phone"),
    (1440, 720, "18:9", "phone & small tablet"),
    // Square, because a tiling-window screenshot rig is a real thing people build.
    (1080, 1080, "1:1", "square"),
];

/// Every standard mode, as `(value, label)` pairs — the device's own orientation first, then
/// the same modes turned the other way.
///
/// Both, because "all the options" means all of them: a phone driving a portrait kiosk layout
/// and a desktop testing a phone layout are both real, and the labels say which is which, so a
/// list containing both is not a trap. The device's orientation leads because it is the one
/// that can fill the screen.
///
/// `exclude` drops sizes already offered above as device-exact, so the same size is never in
/// the list twice with two different labels.
pub fn grouped(
    sw: u32,
    sh: u32,
    exclude: &[String],
) -> (Vec<(String, String)>, Vec<(String, String)>) {
    let portrait = sh > sw;
    (
        one_way(sw, sh, exclude, portrait),
        one_way(sw, sh, exclude, !portrait),
    )
}

/// Every mode turned one way, skipping anything already offered and anything square (which is
/// the same mode both ways round and belongs only to the first group).
fn one_way(sw: u32, sh: u32, exclude: &[String], portrait: bool) -> Vec<(String, String)> {
    let device_way = portrait == (sh > sw);
    let mut out = Vec::new();
    for (long, short, _, class) in MODES {
        if long == short && !device_way {
            continue;
        }
        let (w, h) = if portrait {
            (short, long)
        } else {
            (long, short)
        };
        let value = format!("{w}x{h}");
        if exclude.iter().any(|v| *v == value) {
            continue;
        }
        out.push((value, super::fit::label(sw, sh, w, h, class)));
    }
    out
}

/// Every mode both ways round, flat — for checking that a stored resolution is still on offer.
pub fn options(sw: u32, sh: u32, exclude: &[String]) -> Vec<(String, String)> {
    let (a, b) = grouped(sw, sh, exclude);
    a.into_iter().chain(b).collect()
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
        // The suggestion half: what kind of screen this shape belongs to, and which way round.
        assert!(l.contains("desktop & TV"), "{l}");
        assert!(l.contains("landscape"), "{l}");
    }

    #[test]
    fn both_orientations_are_offered_and_the_devices_leads() {
        let (same, turned) = grouped(2400, 1080, &[]);
        assert!(
            same.iter().all(|(v, _)| {
                let (w, h) = v.split_once('x').unwrap();
                w.parse::<u32>().unwrap() >= h.parse::<u32>().unwrap()
            }),
            "{same:?}"
        );
        assert!(turned.iter().any(|(v, _)| v == "1080x1920"), "{turned:?}");
        // Square is one mode, not two.
        assert!(!turned.iter().any(|(v, _)| v == "1080x1080"), "{turned:?}");
    }

    #[test]
    fn the_device_exact_list_is_not_repeated() {
        let opts = options(2400, 1080, &["1600x720".into()]);
        assert!(!opts.iter().any(|(v, _)| v == "1600x720"), "{opts:?}");
    }
}
