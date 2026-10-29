//! "Pair this device" — shown when the computer refused this device for not knowing it.
//!
//! A computer lets a new device in only with a code shown on it. The QR carries the code in its
//! link and needs no form; this is the other way: the same code, typed — eight characters,
//! `XXXX-XXXX`, case and dashes optional, pasting welcome. Submitting hands it to `js/pair.js`,
//! which proves it inside the envelope and redials.

use dioxus::prelude::*;

use crate::{bridge, state::Ui, ui::widgets::Icon};

/// Characters in a code.
const LEN: usize = 8;
/// Crockford base32: no I, L, O or U.
const ALPHABET: &str = "0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/// What is in the field, as it should read: upper case, look-alikes as the digits they
/// resemble, anything else dropped, at most eight characters, grouped `XXXX-XXXX`.
pub fn format(input: &str) -> String {
    let code: String = input
        .chars()
        .map(|c| match c.to_ascii_uppercase() {
            'I' | 'L' => '1',
            'O' => '0',
            c => c,
        })
        .filter(|c| ALPHABET.contains(*c))
        .take(LEN)
        .collect();
    match code.len() {
        0..=4 => code,
        _ => format!("{}-{}", &code[..4], &code[4..]),
    }
}

/// A whole code is in the field.
fn complete(formatted: &str) -> bool {
    formatted.len() == LEN + 1
}

pub fn render(ui: Ui) -> Element {
    let reason = (ui.live.unpaired)();
    let mut code = use_signal(String::new);
    if reason.is_empty() {
        return rsx! {};
    }
    let ready = complete(&code());
    let mut submit = move || {
        let c = code();
        if complete(&c) {
            bridge::call(format!("window.__wado.pairWith({});", bridge::js(&c)));
            code.set(String::new());
        }
    };
    rsx! {
        form { class: "pairform", "aria-labelledby": "pairtitle",
            onsubmit: move |e| { e.prevent_default(); submit(); },
            div { class: "pairhead",
                span { class: "disc", style: "--hue:var(--base0A)", Icon { name: "link" } }
                b { id: "pairtitle", "Pair this device" }
            }
            p { class: "pairwhy", "{reason}" }
            label { class: "pairlabel", r#for: "paircode", "Code from the computer" }
            div { class: "pairrow",
                input {
                    id: "paircode",
                    r#type: "text",
                    value: "{code}",
                    placeholder: "XXXX-XXXX",
                    autocomplete: "one-time-code",
                    autocapitalize: "characters",
                    autocorrect: "off",
                    spellcheck: "false",
                    inputmode: "text",
                    maxlength: "16",
                    oninput: move |e| code.set(format(&e.value())),
                }
                button { class: "btn primary", r#type: "submit", disabled: !ready, "Pair" }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typing_and_pasting_read_as_the_code() {
        for typed in [
            "k7qf2mxp",
            "K7QF-2MXP",
            "k7qf 2mxp",
            "  k7qf-2mxp  ",
            "K7QF2MXPZZZ",
        ] {
            assert_eq!(format(typed), "K7QF-2MXP", "{typed:?}");
        }
        assert_eq!(
            format("k7q"),
            "K7Q",
            "no dash until the second group starts"
        );
        assert_eq!(format("k7qf"), "K7QF");
        assert_eq!(format("k7qf2"), "K7QF-2");
        assert_eq!(
            format("K7QF-"),
            "K7QF",
            "a trailing dash can be deleted past"
        );
        assert_eq!(
            format("io-lu"),
            "101",
            "look-alikes become digits; U is not a letter here"
        );
        assert!(complete(&format("k7qf2mxp")));
        assert!(!complete(&format("k7qf2mx")));
    }
}
