//! The installed app as the service worker sees it (`public/sw.js`, reported by `js/app_sw.js`):
//! which release is in use, whether it was signed, an update waiting for the next launch, an
//! update refused. And the words for it — the version row in Diagnostics and the update toast.

/// What the worker said last. `state` is `pinned`, `unpinned` (no release on this server — a
/// development build) or `unsupported` (no service worker here); empty until it has spoken.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AppInfo {
    pub state: String,
    pub version: String,
    pub signed: bool,
    /// Signed with a throwaway local key (`release-sign.mjs dev`), only ever on localhost.
    pub dev: bool,
    /// When the release in use was taken into use, as a short date ("30 Oct").
    pub updated: String,
    /// The release key that signed it, as "3f9a 21c0".
    pub key: String,
    /// The version of a verified update waiting for the next launch.
    pub pending: String,
    /// Why the last update was refused, its kind (`unsigned`, `bad`, `hash`, …) and version.
    pub refused: String,
    pub refused_kind: String,
    pub refused_version: String,
    /// Signing is set up (`sw-keys.js` holds a release key): only signed updates are taken. Off,
    /// every update is taken, and Diagnostics says signing still needs setting up.
    pub signing: bool,
}

impl AppInfo {
    pub fn from_msg(msg: &serde_json::Value) -> Self {
        let s = |k: &str| {
            msg.get(k)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string()
        };
        let b = |k: &str| msg.get(k).and_then(|v| v.as_bool()).unwrap_or(false);
        Self {
            state: s("state"),
            version: s("version"),
            signed: b("signed"),
            dev: b("dev"),
            updated: s("updated"),
            key: s("key"),
            pending: s("pending"),
            refused: s("refused"),
            refused_kind: s("refusedKind"),
            refused_version: s("refusedVersion"),
            signing: b("signing"),
        }
    }

    pub fn pinned(&self) -> bool {
        self.state == "pinned"
    }

    /// "wado 0.0.3 · signed ✓ · updated 30 Oct". The version is this build's own until the worker
    /// has named the release it serves.
    pub fn line(&self) -> String {
        let version = if self.version.is_empty() {
            env!("CARGO_PKG_VERSION")
        } else {
            &self.version
        };
        let mut parts = vec![format!("wado {version}")];
        if self.pinned() {
            parts.push(
                match (self.signed, self.dev) {
                    (true, true) => "signed ✓ (dev key)",
                    (true, false) => "signed ✓",
                    _ => "unsigned",
                }
                .to_string(),
            );
            if !self.updated.is_empty() {
                parts.push(format!("updated {}", self.updated));
            }
        }
        parts.join(" · ")
    }

    /// The line under it, in plain words.
    pub fn sub(&self) -> String {
        match self.state.as_str() {
            "pinned" if !self.signing => {
                "Updates aren't signed yet — set up a release key (docs/release-signing.md)".into()
            }
            "pinned" if self.signed => {
                "Opens from this phone, offline too. Updates are taken only when signed.".into()
            }
            "pinned" => {
                "Installed before signing was set up. Updates are taken only when signed.".into()
            }
            "unpinned" => "A development build: loads fresh each time, nothing kept.".into(),
            "unsupported" => "This browser loads the newest version each time.".into(),
            _ => "Checking…".into(),
        }
    }

    /// An update that was refused, in plain words — `None` when none was.
    pub fn refused_words(&self) -> Option<&'static str> {
        if self.refused_kind.is_empty() {
            return None;
        }
        Some(match self.refused_kind.as_str() {
            "unsigned" => "An unsigned update was refused",
            "rollback" => "An older update was refused",
            "unsupported" => "Updates can't be checked in this browser",
            "shape" => "A broken update was refused",
            _ => "An update failed its signature check and was refused",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn line_says_version_signature_and_date() {
        let a = AppInfo::from_msg(&json!({
            "type": "app", "state": "pinned", "version": "0.0.4", "signed": true,
            "updated": "30 Oct", "key": "3f9a 21c0"
        }));
        assert_eq!(a.line(), "wado 0.0.4 · signed ✓ · updated 30 Oct");
        let u = AppInfo::from_msg(&json!({"state": "pinned", "version": "0.0.4"}));
        assert_eq!(u.line(), "wado 0.0.4 · unsigned");
        let d = AppInfo::from_msg(&json!({"state": "unpinned"}));
        assert_eq!(d.line(), format!("wado {}", env!("CARGO_PKG_VERSION")));
    }

    #[test]
    fn refusals_are_named_plainly() {
        let r = |k: &str| AppInfo::from_msg(&json!({"state": "pinned", "refusedKind": k}));
        assert_eq!(
            r("unsigned").refused_words(),
            Some("An unsigned update was refused")
        );
        assert_eq!(
            r("hash").refused_words(),
            Some("An update failed its signature check and was refused")
        );
        assert_eq!(r("").refused_words(), None);
    }

    #[test]
    fn says_when_signing_is_not_set_up() {
        let off = AppInfo::from_msg(&json!({"state": "pinned", "version": "0.0.4"}));
        assert_eq!(
            off.sub(),
            "Updates aren't signed yet — set up a release key (docs/release-signing.md)"
        );
        let on = AppInfo::from_msg(&json!({"state": "pinned", "signing": true, "signed": true}));
        assert_eq!(
            on.sub(),
            "Opens from this phone, offline too. Updates are taken only when signed."
        );
    }
}
