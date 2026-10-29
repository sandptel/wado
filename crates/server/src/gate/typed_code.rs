//! The typed form of a pairing code — for a desktop browser, or when scanning is awkward.
//!
//! Eight characters of Crockford base32 (no I, L, O or U), shown `XXXX-XXXX`, read back case-
//! and dash-insensitively. Forty bits is only safe because guesses are limited: see
//! [`super::Gate::strike`]. The client's twin is `js/pair.js` (`W.pairNormalize`).

/// Characters in a typed code.
pub const LEN: usize = 8;

const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/// A fresh random typed code, normalised (no dash).
pub fn new() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    (0..LEN)
        .map(|_| char::from(ALPHABET[rng.gen_range(0..ALPHABET.len())]))
        .collect()
}

/// What a person typed, as the stored code: separators dropped, upper case, and Crockford's
/// look-alikes read as the digits they resemble (`I`/`L` → `1`, `O` → `0`). `None` unless that
/// leaves exactly [`LEN`] characters of the alphabet.
pub fn normalize(input: &str) -> Option<String> {
    let code: String = input
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .map(|c| match c.to_ascii_uppercase() {
            'I' | 'L' => '1',
            'O' => '0',
            c => c,
        })
        .collect();
    (code.len() == LEN && code.bytes().all(|b| ALPHABET.contains(&b))).then_some(code)
}

/// `XXXX-XXXX`, for a screen.
pub fn display(code: &str) -> String {
    match code.split_at_checked(LEN / 2) {
        Some((a, b)) if code.len() == LEN => format!("{a}-{b}"),
        _ => code.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typing_is_case_dash_and_space_insensitive() {
        for typed in ["k7qf2mxp", "K7QF-2MXP", "k7qf 2mxp", " K7QF - 2mxp "] {
            assert_eq!(normalize(typed).as_deref(), Some("K7QF2MXP"), "{typed:?}");
        }
        assert_eq!(normalize("i0lo-ab12").as_deref(), Some("1010AB12"));
        assert_eq!(normalize("K7QF2MX"), None, "too short");
        assert_eq!(normalize("K7QF2MXPP"), None, "too long");
        assert_eq!(normalize("K7QF2MXU"), None, "U is not in the alphabet");
        assert_eq!(normalize(""), None);
    }

    #[test]
    fn a_new_code_is_normal_and_shown_in_two_groups() {
        let c = new();
        assert_eq!(normalize(&c).as_deref(), Some(c.as_str()));
        assert_ne!(new(), c, "random");
        assert_eq!(display("K7QF2MXP"), "K7QF-2MXP");
    }
}
