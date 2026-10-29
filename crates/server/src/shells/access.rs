//! Who may open, see or type into the shells — a device's `shells` grant.
//!
//! The shells outlive connections and carry whatever the owner left in them (an ssh session, a
//! root prompt), so only a device on the trust list, granted them, gets them.

use crate::gate::Gate;

/// Why `key` may not use the shells, or `None`.
pub fn refused(gate: &Gate, key: &str) -> Option<&'static str> {
    if key.is_empty() || gate.entry(key).is_none() {
        return Some("shells are for paired devices — pair this one with `wado pair`");
    }
    if !gate.grants(key).shells {
        return Some(
            "this device has no shell access — on the computer: wado allow <device> shells",
        );
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strangers_get_no_shells() {
        let dir = std::env::temp_dir().join(format!("wado-shell-access-{}", std::process::id()));
        let g = Gate::at(dir.clone());
        g.trust("owner", "Owner");
        g.trust("phone", "Phone");
        assert!(refused(&g, "owner").is_none(), "the owner may");
        assert!(refused(&g, "phone").is_some(), "not granted");
        g.allow("phone", &["shells"], true).unwrap();
        assert!(refused(&g, "phone").is_none(), "granted");
        assert!(refused(&g, "laptop").is_some(), "not on the list");
        assert!(refused(&g, "").is_some(), "no key");
        let _ = std::fs::remove_dir_all(dir);
    }
}
