//! `window-rule { }` — what to do with an application's windows, by app id or title.
//!
//! ```kdl
//! window-rule {
//!     match app-id="steam"
//!     open-maximized #true
//! }
//! ```

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct WindowRule {
    #[serde(rename = "match")]
    pub matches: Match,
    pub open_maximized: Option<bool>,
}

/// Every field set must match. `app-id` is exact; `title` matches anywhere in the title.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Match {
    pub app_id: Option<String>,
    pub title: Option<String>,
}

impl Match {
    pub fn hits(&self, app_id: &str, title: &str) -> bool {
        (self.app_id.is_some() || self.title.is_some())
            && self.app_id.as_deref().is_none_or(|a| a == app_id)
            && self.title.as_deref().is_none_or(|t| title.contains(t))
    }
}

/// The first rule that matches decides, as in niri.
pub fn first<'a>(rules: &'a [WindowRule], app_id: &str, title: &str) -> Option<&'a WindowRule> {
    rules.iter().find(|r| r.matches.hits(app_id, title))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rules_match_and_an_empty_match_matches_nothing() {
        let r = crate::kdl::parse("window-rule { match app-id=\"steam\"; open-maximized #true; }\nwindow-rule { match title=\"Picture\"; }\nwindow-rule { open-maximized #true; }").unwrap();
        assert_eq!(r.window_rule.len(), 3);
        assert_eq!(
            first(&r.window_rule, "steam", "").and_then(|r| r.open_maximized),
            Some(true)
        );
        assert!(first(&r.window_rule, "firefox", "Picture-in-Picture").is_some());
        assert!(first(&r.window_rule, "foot", "zsh").is_none());
    }
}
