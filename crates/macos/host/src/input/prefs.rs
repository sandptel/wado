//! What this Mac's own trackpad does with three and four fingers, read from System Settings ›
//! Trackpad, so the phone's pad does the same. Read each time the pad opens.

use std::collections::HashMap;

/// A horizontal swipe: nothing, back and forward between pages, or between Spaces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Horiz {
    Off,
    Pages,
    Spaces,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Settings {
    pub three_h: Horiz,
    /// Mission Control / App Exposé.
    pub three_v: bool,
    pub four_h: Horiz,
    pub four_v: bool,
    /// Four (or five) fingers pinching: Launchpad and Show Desktop.
    pub pinch: bool,
}

impl Default for Settings {
    /// A new Mac's.
    fn default() -> Self {
        Settings {
            three_h: Horiz::Spaces,
            three_v: true,
            four_h: Horiz::Spaces,
            four_v: true,
            pinch: true,
        }
    }
}

impl Settings {
    /// The horizontal swipe for `n` fingers.
    pub fn horiz(&self, n: usize) -> Horiz {
        if n == 3 { self.three_h } else { self.four_h }
    }

    /// Whether `n` fingers up or down is Mission Control.
    pub fn vert(&self, n: usize) -> bool {
        if n == 3 { self.three_v } else { self.four_v }
    }
}

/// Settings from `defaults read com.apple.AppleMultitouchTrackpad`; a key it lacks keeps a new
/// Mac's default.
pub fn parse(out: &str) -> Settings {
    let v: HashMap<&str, i64> = out
        .lines()
        .filter_map(|l| {
            let (k, n) = l.trim().trim_end_matches(';').split_once(" = ")?;
            Some((k.trim(), n.trim().parse().ok()?))
        })
        .collect();
    let d = Settings::default();
    let horiz = |k: &str, d: Horiz| match v.get(k) {
        Some(0) => Horiz::Off,
        Some(1) => Horiz::Pages,
        Some(_) => Horiz::Spaces,
        None => d,
    };
    let on = |k: &str, d: bool| v.get(k).map_or(d, |n| *n != 0);
    Settings {
        three_h: horiz("TrackpadThreeFingerHorizSwipeGesture", d.three_h),
        three_v: on("TrackpadThreeFingerVertSwipeGesture", d.three_v),
        four_h: horiz("TrackpadFourFingerHorizSwipeGesture", d.four_h),
        four_v: on("TrackpadFourFingerVertSwipeGesture", d.four_v),
        pinch: on("TrackpadFourFingerPinchGesture", d.pinch),
    }
}

pub fn read() -> Settings {
    std::process::Command::new("/usr/bin/defaults")
        .args(["read", "com.apple.AppleMultitouchTrackpad"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| parse(&String::from_utf8_lossy(&o.stdout)))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_trackpad_pane() {
        let s = parse(
            "{\n    TrackpadFourFingerHorizSwipeGesture = 2;\n    TrackpadThreeFingerHorizSwipeGesture = 1;\n    \
             TrackpadThreeFingerVertSwipeGesture = 0;\n    TrackpadFourFingerPinchGesture = 0;\n    version = 12;\n}\n",
        );
        assert_eq!(
            s,
            Settings {
                three_h: Horiz::Pages,
                three_v: false,
                four_h: Horiz::Spaces,
                four_v: true,
                pinch: false,
            }
        );
        assert_eq!(s.horiz(4), Horiz::Spaces);
        assert_eq!(parse(""), Settings::default());
    }
}
