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
}
