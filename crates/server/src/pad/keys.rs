//! What a key message means as a key: the named keys and the US-layout characters, as evdev
//! `KEY_*` codes — the platform layer's lingua franca (`wado_platform::input`). The device that
//! presses them is the platform's (`crate::platform::linux::keyboard` here).

/// `KEY_*` codes for the named keys `PadMsg::Key` carries (kernel header `linux/input-event-codes.h`).
pub fn key_code(name: &str) -> Option<u16> {
    Some(match name {
        "esc" => 1,
        "tab" => 15,
        "enter" => 28,
        "backspace" => 14,
        "delete" => 111,
        "left" => 105,
        "right" => 106,
        "up" => 103,
        "down" => 108,
        "home" => 102,
        "end" => 107,
        "pageup" => 104,
        "pagedown" => 109,
        "space" => 57,
        "f1" => 59,
        "f2" => 60,
        "f3" => 61,
        "f4" => 62,
        "f5" => 63,
        "f6" => 64,
        "f7" => 65,
        "f8" => 66,
        "f9" => 67,
        "f10" => 68,
        "f11" => 87,
        "f12" => 88,
        _ => return None,
    })
}

/// ponytail: US table only — non-US hosts get the wrong punctuation out of `char_code`; the
/// upgrade is an xkbcommon lookup of the host's actual keymap, not a bigger match arm here.
pub fn char_code(ch: char) -> Option<(u16, bool)> {
    let (code, shift) = match ch {
        'a'..='z' => (letter_code(ch), false),
        'A'..='Z' => (letter_code(ch.to_ascii_lowercase()), true),
        '1' => (2, false),
        '2' => (3, false),
        '3' => (4, false),
        '4' => (5, false),
        '5' => (6, false),
        '6' => (7, false),
        '7' => (8, false),
        '8' => (9, false),
        '9' => (10, false),
        '0' => (11, false),
        '!' => (2, true),
        '@' => (3, true),
        '#' => (4, true),
        '$' => (5, true),
        '%' => (6, true),
        '^' => (7, true),
        '&' => (8, true),
        '*' => (9, true),
        '(' => (10, true),
        ')' => (11, true),
        ' ' => (57, false),
        '\n' => (28, false),
        '\t' => (15, false),
        '-' => (12, false),
        '_' => (12, true),
        '=' => (13, false),
        '+' => (13, true),
        '[' => (26, false),
        '{' => (26, true),
        ']' => (27, false),
        '}' => (27, true),
        '\\' => (43, false),
        '|' => (43, true),
        ';' => (39, false),
        ':' => (39, true),
        '\'' => (40, false),
        '"' => (40, true),
        '`' => (41, false),
        '~' => (41, true),
        ',' => (51, false),
        '<' => (51, true),
        '.' => (52, false),
        '>' => (52, true),
        '/' => (53, false),
        '?' => (53, true),
        _ => return None,
    };
    Some((code, shift))
}

/// `KEY_*` code for one lowercase `a..=z`, laid out as the US QWERTY rows.
fn letter_code(ch: char) -> u16 {
    match ch {
        'q' => 16,
        'w' => 17,
        'e' => 18,
        'r' => 19,
        't' => 20,
        'y' => 21,
        'u' => 22,
        'i' => 23,
        'o' => 24,
        'p' => 25,
        'a' => 30,
        's' => 31,
        'd' => 32,
        'f' => 33,
        'g' => 34,
        'h' => 35,
        'j' => 36,
        'k' => 37,
        'l' => 38,
        'z' => 44,
        'x' => 45,
        'c' => 46,
        'v' => 47,
        'b' => 48,
        'n' => 49,
        'm' => 50,
        _ => unreachable!("letter_code called with a non a..=z char"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn us_table() {
        assert_eq!(char_code('a'), Some((30, false)));
        assert_eq!(char_code('A'), Some((30, true)));
        assert_eq!(char_code('1'), Some((2, false)));
        assert_eq!(char_code('!'), Some((2, true)));
        assert_eq!(char_code(' '), Some((57, false)));
        assert_eq!(char_code('é'), None);
        assert_eq!(key_code("esc"), Some(1));
        assert_eq!(key_code("nope"), None);
    }
}
