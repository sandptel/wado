//! Linux evdev `KEY_*` codes — the platform layer's key names — as macOS virtual key codes
//! (`kVK_*`, Carbon `Events.h`), for a US layout.

/// The Mac key for evdev `code`, when it has one.
pub fn kvk(code: u16) -> Option<u16> {
    Some(match code {
        1 => 53,    // ESC
        2 => 18,    // 1
        3 => 19,    // 2
        4 => 20,    // 3
        5 => 21,    // 4
        6 => 23,    // 5
        7 => 22,    // 6
        8 => 26,    // 7
        9 => 28,    // 8
        10 => 25,   // 9
        11 => 29,   // 0
        12 => 27,   // -
        13 => 24,   // =
        14 => 51,   // BACKSPACE
        15 => 48,   // TAB
        16 => 12,   // Q
        17 => 13,   // W
        18 => 14,   // E
        19 => 15,   // R
        20 => 17,   // T
        21 => 16,   // Y
        22 => 32,   // U
        23 => 34,   // I
        24 => 31,   // O
        25 => 35,   // P
        26 => 33,   // [
        27 => 30,   // ]
        28 => 36,   // ENTER
        29 => 59,   // LEFTCTRL
        30 => 0,    // A
        31 => 1,    // S
        32 => 2,    // D
        33 => 3,    // F
        34 => 5,    // G
        35 => 4,    // H
        36 => 38,   // J
        37 => 40,   // K
        38 => 37,   // L
        39 => 41,   // ;
        40 => 39,   // '
        41 => 50,   // `
        42 => 56,   // LEFTSHIFT
        43 => 42,   // backslash
        44 => 6,    // Z
        45 => 7,    // X
        46 => 8,    // C
        47 => 9,    // V
        48 => 11,   // B
        49 => 45,   // N
        50 => 46,   // M
        51 => 43,   // ,
        52 => 47,   // .
        53 => 44,   // /
        54 => 60,   // RIGHTSHIFT
        55 => 67,   // KP*
        56 => 58,   // LEFTALT
        57 => 49,   // SPACE
        58 => 57,   // CAPSLOCK
        59 => 122,  // F1
        60 => 120,  // F2
        61 => 99,   // F3
        62 => 118,  // F4
        63 => 96,   // F5
        64 => 97,   // F6
        65 => 98,   // F7
        66 => 100,  // F8
        67 => 101,  // F9
        68 => 109,  // F10
        87 => 103,  // F11
        88 => 111,  // F12
        183 => 105, // F13
        184 => 107, // F14
        185 => 113, // F15
        186 => 106, // F16
        187 => 64,  // F17
        188 => 79,  // F18
        189 => 80,  // F19
        190 => 90,  // F20
        71 => 89,   // KP7
        72 => 91,   // KP8
        73 => 92,   // KP9
        74 => 78,   // KP-
        75 => 86,   // KP4
        76 => 87,   // KP5
        77 => 88,   // KP6
        78 => 69,   // KP+
        79 => 83,   // KP1
        80 => 84,   // KP2
        81 => 85,   // KP3
        82 => 82,   // KP0
        83 => 65,   // KP.
        96 => 76,   // KPENTER
        97 => 62,   // RIGHTCTRL
        98 => 75,   // KP/
        100 => 61,  // RIGHTALT
        102 => 115, // HOME
        103 => 126, // UP
        104 => 116, // PAGEUP
        105 => 123, // LEFT
        106 => 124, // RIGHT
        107 => 119, // END
        108 => 125, // DOWN
        109 => 121, // PAGEDOWN
        110 => 114, // INSERT → Help
        111 => 117, // DELETE (forward)
        113 => 74,  // MUTE
        114 => 73,  // VOLUMEDOWN
        115 => 72,  // VOLUMEUP
        117 => 81,  // KPEQUAL
        125 => 55,  // LEFTMETA → Command
        126 => 54,  // RIGHTMETA → right Command
        _ => return None,
    })
}

/// Keys a Mac keyboard sends with the function and keypad flags set (arrows and the navigation
/// cluster) — posted without them, some shortcuts that use arrows are not recognised.
pub fn nav_flags(kvk: u16) -> u64 {
    use super::post::flag;
    match kvk {
        123..=126 => flag::FN | flag::NUMPAD,
        114..=117 | 119 | 121 => flag::FN,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kvk_maps_letters_and_modifiers() {
        assert_eq!(kvk(30), Some(0)); // A
        assert_eq!(kvk(28), Some(36)); // Enter
        assert_eq!(kvk(125), Some(55)); // Meta → Command
        assert_eq!(kvk(29), Some(59)); // Ctrl
        assert_eq!(kvk(103), Some(126)); // Up
        assert_eq!(kvk(999), None);
    }

    #[test]
    fn every_mapped_key_is_distinct() {
        let mut seen = std::collections::HashMap::new();
        for code in 0..300u16 {
            if let Some(k) = kvk(code) {
                assert!(
                    seen.insert(k, code).is_none(),
                    "kVK {k} twice ({code} and {:?})",
                    seen.get(&k)
                );
            }
        }
    }
}
