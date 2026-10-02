//! `binds { }` — keyboard shortcuts the compositor keeps for itself.
//!
//! ```kdl
//! binds {
//!     mod "super"            // what `Mod` means: super | alt | ctrl | ctrl+alt
//!     Mod+Q "close-window"
//!     Mod+Tab "focus-next"
//! }
//! ```
//!
//! `Mod` is configurable because a browser cannot capture Super on every OS — the system takes
//! it first — so a phone with a keyboard may need `ctrl+alt` instead.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// What a bind can do: the window actions the compositor already has. No `spawn`: a bind that
/// runs commands would make this section privileged, and the drawer already launches.
pub const ACTIONS: &[&str] = &["close-window", "maximize", "minimize", "focus-next", "back"];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Binds {
    #[serde(rename = "mod")]
    pub modifier: String,
    /// `"Mod+Q"` → `"close-window"`.
    #[serde(flatten)]
    pub keys: BTreeMap<String, String>,
}

impl Default for Binds {
    fn default() -> Self {
        Self {
            modifier: "super".into(),
            keys: BTreeMap::new(),
        }
    }
}

/// A parsed key combination: modifiers, and the key as an xkb keysym name.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Combo {
    pub logo: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub key: String,
}

impl Combo {
    /// `"Mod+Shift+Q"` with `Mod` meaning `modifier`.
    pub fn parse(text: &str, modifier: &str) -> Result<Self, String> {
        let mut parts: Vec<&str> = text.split('+').map(str::trim).collect();
        let key = parts
            .pop()
            .filter(|k| !k.is_empty())
            .ok_or("no key after the modifiers")?;
        let mut c = Combo {
            key: key.to_string(),
            ..Default::default()
        };
        let mut set = |m: &str| -> Result<(), String> {
            match m.to_ascii_lowercase().as_str() {
                "super" | "logo" | "win" => c.logo = true,
                "ctrl" | "control" => c.ctrl = true,
                "alt" => c.alt = true,
                "shift" => c.shift = true,
                other => return Err(format!("unknown modifier `{other}`")),
            }
            Ok(())
        };
        for m in parts {
            if m.eq_ignore_ascii_case("mod") {
                for part in modifier.split('+') {
                    set(part)?;
                }
            } else {
                set(m)?;
            }
        }
        if !(c.logo || c.ctrl || c.alt) {
            return Err(
                "a bind needs Mod, Super, Ctrl or Alt — a bare key would never reach an app".into(),
            );
        }
        Ok(c)
    }
}

impl Binds {
    /// Every bind, parsed. The first bad one is the error, as `(key, message)`.
    pub fn parsed(&self) -> Result<Vec<(Combo, String)>, (String, String)> {
        self.keys
            .iter()
            .map(|(k, action)| {
                let combo = Combo::parse(k, &self.modifier).map_err(|e| (k.clone(), e))?;
                if !ACTIONS.contains(&action.as_str()) {
                    return Err((
                        k.clone(),
                        format!(
                            "unknown action `{action}`, expected one of {}",
                            ACTIONS.join(", ")
                        ),
                    ));
                }
                Ok((combo, action.clone()))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mod_expands_and_bare_keys_are_refused() {
        let c = Combo::parse("Mod+Shift+q", "ctrl+alt").unwrap();
        assert!(c.ctrl && c.alt && c.shift && !c.logo);
        assert_eq!(c.key, "q");
        assert!(Combo::parse("Shift+q", "super").is_err());
        assert!(Combo::parse("Hyper+q", "super").is_err());
    }
}
