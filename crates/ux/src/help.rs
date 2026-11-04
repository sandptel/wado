//! The `?` overlay's rows: what every gesture and control does *here*, from the device's real
//! settings, so it never describes something that is off. Different per form: a phone and a tablet
//! are touch; a desktop is a mouse and a keyboard, where the computer's own shortcuts matter.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::form::Form;

/// What the rows are built from — the device's settings and the computer's config.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Input {
    /// `home`, `picture` or `pad`.
    pub surface: String,
    pub form: Form,
    /// Touches go to the app as touches (`touch_mode: touch`).
    pub raw: bool,
    pub swipe3: bool,
    pub edge_swipe: bool,
    /// A Game mode is shadowing the settings.
    pub game: bool,
    /// The computer's three-finger mappings: `swipe-3-up` → `app-drawer`.
    pub gestures: BTreeMap<String, String>,
    /// The computer's own keyboard shortcuts: `Mod+Q` → `close-window`, and what `Mod` is.
    pub binds: BTreeMap<String, String>,
    pub bind_mod: String,
}

/// One row: how many fingers (0: none drawn), an icon name both clients have, the words.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Row {
    pub fingers: u8,
    pub icon: String,
    pub words: String,
}

fn row(fingers: u8, icon: &str, words: impl Into<String>) -> Row {
    Row {
        fingers,
        icon: icon.into(),
        words: words.into(),
    }
}

fn action(a: &str) -> String {
    match a {
        "app-drawer" => "your apps".into(),
        "control-centre" => "the controls".into(),
        "back" => "Back".into(),
        "focus-next" => "the next window".into(),
        "keyboard" => "the keyboard".into(),
        "workspace-next" => "the next workspace".into(),
        "workspace-prev" => "the last workspace".into(),
        "close-window" => "close the window".into(),
        o => o.replace('-', " "),
    }
}

pub fn rows(i: &Input) -> Vec<Row> {
    let mut out = Vec::new();
    if i.surface == "home" {
        out.push(row(
            1,
            "",
            "Tap a game to play it, ready the way you set it up",
        ));
        out.push(row(
            1,
            "pin",
            if i.form.touch() {
                "Hold a game for its Game mode settings, or to turn it off"
            } else {
                "Game mode under a game: its settings, or turn it off"
            },
        ));
        out.push(row(1, "plus", "Add a game: turn on Game mode for any app"));
        out.push(row(
            1,
            "play",
            "▶ on a computer starts or rejoins its desktop",
        ));
        out.push(row(
            1,
            "down",
            "The name at the top switches computers, or shows them all",
        ));
        if i.form == Form::Phone {
            out.push(row(1, "camera", "Scan a code to add a computer"));
        }
        return out;
    }
    if i.surface == "pad" {
        out.push(row(
            0,
            "pad",
            "The buttons press the computer's gamepad. ⋯ changes the layout",
        ));
    }
    if i.form == Form::Desktop {
        out.push(row(
            0,
            "keys",
            "Your mouse and keyboard go straight to the computer",
        ));
        out.push(row(
            0,
            "hand",
            "Right-click and the scroll wheel work as they do there",
        ));
        let m = if i.bind_mod.is_empty() {
            "Mod".to_string()
        } else {
            i.bind_mod.clone()
        };
        for (k, a) in i.binds.iter().take(4) {
            out.push(row(
                0,
                "kbd",
                format!("{}: {}", k.replace("Mod", &m), action(a)),
            ));
        }
        out.push(row(
            0,
            "max",
            "Full screen from the rail; its button brings the console back",
        ));
        return out;
    }
    if i.raw {
        out.push(row(1, "", "Touches go to the app as touches"));
    } else {
        out.push(row(1, "", "Tap to click"));
        out.push(row(1, "down", "Drag one finger to scroll"));
        out.push(row(1, "pin", "Hold for a right-click, or to drag"));
        out.push(row(2, "max", "Pinch with two fingers to zoom"));
    }
    if i.swipe3 {
        for (d, dflt) in [
            ("up", "app-drawer"),
            ("down", "control-centre"),
            ("left", "back"),
            ("right", "focus-next"),
        ] {
            let a = i
                .gestures
                .get(&format!("swipe-3-{d}"))
                .map_or(dflt, String::as_str);
            if a != "none" {
                out.push(row(3, d, format!("Three fingers {d}: {}", action(a))));
            }
        }
    }
    if i.edge_swipe {
        out.push(row(
            1,
            "right",
            "Swipe in from a side edge: the next workspace",
        ));
    }
    if i.form == Form::Tablet {
        out.push(row(
            0,
            "kbd",
            "A keyboard or mouse plugged into the tablet works too",
        ));
    }
    if i.game && !i.swipe3 {
        out.push(row(
            0,
            "lock",
            "Game mode has the swipes off, so a game never catches one by accident",
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_never_lists_what_is_off_and_fits_the_form() {
        let mut i = Input {
            surface: "picture".into(),
            swipe3: true,
            ..Default::default()
        };
        i.gestures.insert("swipe-3-left".into(), "none".into());
        let r = rows(&i);
        assert!(r.iter().any(|x| x.words == "Three fingers up: your apps"));
        assert!(
            !r.iter().any(|x| x.words.contains("fingers left")),
            "mapped to none: not listed"
        );
        assert!(
            !r.iter().any(|x| x.words.contains("side edge")),
            "edge swipes off: not listed"
        );
        i.form = Form::Desktop;
        i.binds.insert("Mod+Q".into(), "close-window".into());
        i.bind_mod = "Super".into();
        let d = rows(&i);
        assert!(d.iter().any(|x| x.words == "Super+Q: close the window"));
        assert!(
            !d.iter().any(|x| x.fingers > 0),
            "a desktop has no fingers to draw"
        );
    }
}
