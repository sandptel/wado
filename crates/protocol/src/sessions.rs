//! A running session, as the home page lists it — whichever daemon of the pool it is on.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionSummary {
    /// The relay's name for the daemon it runs on (`<remote id>:<instance>`), which is what a
    /// join asks for to land on it.
    pub instance: String,
    /// When it started, unix ms.
    pub started_ms: u64,
    /// How long it has been running, seconds — filled in when listed, so a browser need not
    /// trust its own clock against the computer's.
    #[serde(default)]
    pub age_s: u64,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    /// Its windows' app ids (or titles), in order.
    pub apps: Vec<String>,
    /// The device watching it right now, if any.
    #[serde(default)]
    pub viewer: Option<String>,
    /// Left running on purpose ("Leave"), so it is kept until someone ends it.
    #[serde(default)]
    pub detached: bool,
    /// The device it is sized for: it started the session or last took it over. `None` from a
    /// daemon before multi-client sessions.
    #[serde(default)]
    pub lead: Option<String>,
    /// Every device in it now, the lead first.
    #[serde(default)]
    pub participants: Vec<crate::multi::Participant>,
    /// How many devices it takes. `0`: a daemon from before multi-client sessions — it can be
    /// resumed or taken over, never joined.
    #[serde(default)]
    pub max_participants: u32,
    /// The shared stream's bitrate now, kbps (the lowest any participant can take, floored).
    #[serde(default)]
    pub kbps: u32,
}

#[cfg(test)]
mod tests {
    use super::SessionSummary;

    #[test]
    fn a_file_from_an_older_daemon_still_parses() {
        let old = r#"{"instance":"1:1","started_ms":5,"width":1080,"height":2400,"fps":60,
            "apps":["foot"],"viewer":"Pixel","detached":false}"#;
        let s: SessionSummary = serde_json::from_str(old).unwrap();
        assert_eq!(s.max_participants, 0, "old daemon: not joinable");
        assert!(s.participants.is_empty() && s.lead.is_none());
    }
}
