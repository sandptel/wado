//! `gestures { }` — what three-finger swipes do, recognised on the client.

use std::collections::BTreeMap;

/// The swipes the client recognises.
pub const NAMES: &[&str] = &[
    "swipe-3-up",
    "swipe-3-down",
    "swipe-3-left",
    "swipe-3-right",
];

/// What a swipe can do: a window action, or a part of the client.
pub const ACTIONS: &[&str] = &[
    "app-drawer",
    "control-centre",
    "keyboard",
    "back",
    "focus-next",
    "close-window",
    "maximize",
    "minimize",
    "none",
];

pub fn defaults() -> BTreeMap<String, String> {
    [
        ("swipe-3-up", "app-drawer"),
        ("swipe-3-down", "control-centre"),
        ("swipe-3-left", "back"),
        ("swipe-3-right", "focus-next"),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v.to_string()))
    .collect()
}

pub fn check(g: &BTreeMap<String, String>) -> Result<(), (String, String)> {
    for (k, v) in g {
        if !NAMES.contains(&k.as_str()) {
            return Err((
                k.clone(),
                format!(
                    "unknown gesture `{k}`, expected one of {}",
                    NAMES.join(", ")
                ),
            ));
        }
        if !ACTIONS.contains(&v.as_str()) {
            return Err((
                k.clone(),
                format!(
                    "unknown action `{v}`, expected one of {}",
                    ACTIONS.join(", ")
                ),
            ));
        }
    }
    Ok(())
}
