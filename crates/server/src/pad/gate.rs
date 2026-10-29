//! Who may use the trackpad. Asked of every message, not once at open, so a grant revoked from
//! `wado tui` stops the next touch.

use crate::files::channel::Who;
use crate::gate::Gate;

/// Why `who` may not drive this computer's input, or `None` when it may.
pub fn refused(gate: &Gate, who: &Who, approved: bool) -> Option<String> {
    if !approved {
        return Some("this device has not been approved yet".into());
    }
    if !wado_config::live::current().trackpad.enabled {
        return Some("the trackpad is turned off on this computer (trackpad { enabled })".into());
    }
    if !gate.grants(&who.key).input {
        return Some(format!(
            "this device may not use the trackpad — on the computer: wado allow {} input",
            Gate::typed(&who.name)
        ));
    }
    None
}

/// Why `who` may not list or run actions (`crate::actions`), or `None` when it may. Actions are
/// the computer's own controls — the `host` grant — not input, so a device may have either.
pub fn refused_host(gate: &Gate, who: &Who, approved: bool) -> Option<String> {
    if !approved {
        return Some("this device has not been approved yet".into());
    }
    if !gate.grants(&who.key).host {
        return Some(format!(
            "this device may not run actions — on the computer: wado allow {} host",
            Gate::typed(&who.name)
        ));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gate() -> Gate {
        let dir = std::env::temp_dir().join(format!("wado-pad-gate-{}", rand::random::<u64>()));
        std::fs::create_dir_all(&dir).unwrap();
        let g = Gate::at(dir);
        g.trust("k0", "laptop"); // the first device is the owner; keep it out of the way
        g.trust("k1", "phone");
        g.trust("k2", "tab");
        g.allow("phone", &["host"], true).unwrap();
        g.allow("tab", &["input"], true).unwrap();
        g
    }
    fn who(key: &str, name: &str) -> Who {
        Who {
            key: key.into(),
            name: name.into(),
        }
    }

    #[test]
    fn needs_input_not_host() {
        let g = gate();
        assert!(
            refused(&g, &who("k1", "phone"), true)
                .unwrap()
                .contains("wado allow phone input")
        );
        assert!(refused(&g, &who("k2", "tab"), true).is_none());
        assert!(
            refused(&g, &who("k2", "tab"), false)
                .unwrap()
                .contains("not been approved")
        );
    }

    #[test]
    fn actions_need_host_not_input() {
        let g = gate();
        g.allow("tab", &["host"], false).unwrap();
        assert!(refused_host(&g, &who("k1", "phone"), true).is_none());
        let why = refused_host(&g, &who("k2", "tab"), true).unwrap();
        assert_eq!(
            why,
            "this device may not run actions — on the computer: wado allow tab host"
        );
        assert!(
            refused_host(&g, &who("k1", "phone"), false)
                .unwrap()
                .contains("not been approved")
        );
        let quoted = refused_host(&g, &who("k2", "Android · Chrome"), true).unwrap();
        assert!(
            quoted.ends_with("wado allow \"Android · Chrome\" host"),
            "{quoted}"
        );
    }

    #[test]
    fn a_name_with_spaces_is_quoted_ready_to_paste() {
        let g = gate();
        let why = refused(&g, &who("k1", "Android · Chrome"), true).unwrap();
        assert!(
            why.ends_with("wado allow \"Android · Chrome\" input"),
            "{why}"
        );
    }

    #[test]
    fn revoked_grant_refuses_next_message() {
        let g = gate();
        assert!(refused(&g, &who("k2", "tab"), true).is_none());
        g.allow("tab", &["input"], false).unwrap();
        assert!(refused(&g, &who("k2", "tab"), true).is_some());
    }
}
