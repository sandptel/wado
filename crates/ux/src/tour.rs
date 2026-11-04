//! The first-run tour: four cards after the first pairing, worded for the form in hand.

use serde::{Deserialize, Serialize};

use crate::form::Form;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Card {
    /// The base16 slot its wash takes.
    pub hue: u8,
    /// `shelf`, or an icon name.
    pub art: String,
    pub title: String,
    pub words: String,
}

pub fn cards(computer: &str, form: Form) -> Vec<Card> {
    let c = |hue: u8, art: &str, title: String, words: String| Card {
        hue,
        art: art.into(),
        title,
        words,
    };
    let screen = match form {
        Form::Phone => "this phone",
        Form::Tablet => "this tablet",
        Form::Desktop => "this screen",
    };
    let help = if form.touch() {
        "Tap it in any session to see what every gesture and button does on that screen."
    } else {
        "Click it in any session to see what the mouse, the keys and the computer's shortcuts do there."
    };
    vec![
        c(0xE, "shelf", format!("{computer}'s games and apps, right here"), format!("Everything installed on {computer} can open on {screen}, at its own size.")),
        c(0xD, "pad", "Set a game up once".into(), "Turn on Game mode for a game and say how you play. Every time after, one tap opens it ready.".into()),
        c(0xA, "?", "? is always there".into(), help.into()),
        c(0xB, "lock", "It's still your computer".into(), format!("What {screen} may do is decided on {computer}. When something is off, wado says why and how to change it.")),
    ]
}
