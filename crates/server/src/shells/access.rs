//! Who may open, see or type into the shells — `security { shell-access }`.
//!
//! The shells outlive connections and carry whatever the owner left in them (an ssh session, a
//! root prompt), so a device let in once — by a tap, or by `join "open"` — never gets them.

use crate::gate::Gate;

/// Why `key` may not use the shells, or `None`. `once`: it was let in for this visit only.
pub fn refused(gate: &Gate, key: &str, once: bool) -> Option<&'static str> {
    if once || key.is_empty() || gate.entry(key).is_none() {
        return Some("shells are for trusted devices — this one was let in once");
    }
    if wado_config::live::current().security.owner_only_shells()
        && !crate::config::link::is_owner(key, gate)
    {
        return Some("shells are for this computer's owner device (security { shell-access })");
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn once_and_strangers_get_no_shells() {
        let dir = std::env::temp_dir().join(format!("wado-shell-access-{}", std::process::id()));
        let g = Gate::at(dir.clone());
        g.trust("phone", "Phone");
        assert!(refused(&g, "phone", false).is_none(), "trusted, by default");
        assert!(refused(&g, "phone", true).is_some(), "let in once");
        assert!(refused(&g, "laptop", false).is_some(), "not on the list");
        assert!(refused(&g, "", false).is_some(), "no key");
        let _ = std::fs::remove_dir_all(dir);
    }
}
