//! A computer's OS logo, from os-release: exact `ID`, then each `ID_LIKE` entry in order, then Tux.
//! Slugs are Simple Icons slugs; `scripts/os-logos.mjs` makes one drawable per slug in [`slugs`].
//! Only CC0 icons are used: NixOS, Debian, Fedora, Gentoo, Rocky and MX carry other licences in
//! Simple Icons, so they fall back on `ID_LIKE` (Rocky → Red Hat) or Tux until that is decided.
//! Pure, and shared with the desktop app.

/// os-release `ID` → Simple Icons slug.
const IDS: &[(&str, &str)] = &[
    ("arch", "archlinux"),
    ("endeavouros", "endeavouros"),
    ("manjaro", "manjaro"),
    ("ubuntu", "ubuntu"),
    ("pop", "popos"),
    ("linuxmint", "linuxmint"),
    ("opensuse", "opensuse"),
    ("alpine", "alpinelinux"),
    ("elementary", "elementary"),
    ("kali", "kalilinux"),
    ("void", "voidlinux"),
    ("centos", "centos"),
    ("almalinux", "almalinux"),
    ("rhel", "redhat"),
    ("zorin", "zorin"),
    ("artix", "artixlinux"),
    ("deepin", "deepin"),
    ("solus", "solus"),
    ("steamos", "steam"),
];

/// The fallback: Tux.
pub const LINUX: &str = "linux";

/// Every slug [`slug`] can return, the fallback included.
pub fn slugs() -> Vec<&'static str> {
    let mut v: Vec<&'static str> = IDS.iter().map(|(_, s)| *s).collect();
    v.push(LINUX);
    v.sort_unstable();
    v.dedup();
    v
}

fn one(id: &str) -> Option<&'static str> {
    let id = id.trim().to_ascii_lowercase();
    // openSUSE's IDs are `opensuse-tumbleweed`, `opensuse-leap`, …
    let id = if id.starts_with("opensuse") {
        "opensuse"
    } else {
        id.as_str()
    };
    IDS.iter().find(|(k, _)| *k == id).map(|(_, s)| *s)
}

/// The logo slug for `os_id` and `os_like`.
pub fn slug(os_id: &str, os_like: &[String]) -> &'static str {
    one(os_id)
        .or_else(|| os_like.iter().find_map(|l| one(l)))
        .unwrap_or(LINUX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_id_wins() {
        assert_eq!(slug("pop", &["ubuntu".into(), "debian".into()]), "popos");
        assert_eq!(slug("ubuntu", &[]), "ubuntu");
        assert_eq!(slug("Arch", &[]), "archlinux");
    }

    #[test]
    fn id_like_order_is_followed() {
        assert_eq!(slug("cachyos", &["arch".into()]), "archlinux");
        assert_eq!(slug("neon", &["ubuntu".into(), "debian".into()]), "ubuntu");
    }

    #[test]
    fn opensuse_family() {
        assert_eq!(slug("opensuse-tumbleweed", &[]), "opensuse");
        assert_eq!(slug("x", &["opensuse-leap".into()]), "opensuse");
    }

    #[test]
    fn unknown_falls_back_to_linux() {
        assert_eq!(slug("", &[]), "linux");
        assert_eq!(slug("mystery", &["nothing".into()]), "linux");
    }

    /// Every slug has a drawable in the app (made by `scripts/os-logos.mjs`).
    #[test]
    fn every_slug_has_a_drawable() {
        let res = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../android/app/src/main/res/drawable");
        for s in slugs() {
            let f = res.join(format!("os_{s}.xml"));
            assert!(f.exists(), "missing {}", f.display());
        }
    }
}
