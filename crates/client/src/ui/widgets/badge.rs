//! When a setting takes effect. On the control rather than on the page, because pages are now
//! grouped by subject and one page mixes all three.

use dioxus::prelude::*;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum When {
    /// Changes the running session through Apply.
    Apply,
    /// Only a new session picks it up.
    Restart,
    /// Set by the host's config.kdl; the client cannot change it here.
    Host,
}

#[component]
pub fn WhenBadge(when: When) -> Element {
    let (class, text) = match when {
        When::Apply => ("whenbadge apply", "on apply"),
        When::Restart => ("whenbadge restart", "next start"),
        When::Host => ("whenbadge host", "set by host"),
    };
    rsx! { span { class, "{text}" } }
}
