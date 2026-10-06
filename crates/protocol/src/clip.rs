//! The computer's clipboard history, as the landing page's clipboard rail shows it — the host
//! desktop's own clipboard (cliphist), kept in step with the session's.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClipKind {
    Text,
    Link,
    Image,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClipEntry {
    /// cliphist's id; stable while the entry is in history.
    pub id: String,
    pub kind: ClipKind,
    /// The text, trimmed for a tile; for an image, cliphist's description (`png 1920x1080`).
    pub preview: String,
    #[serde(default)]
    pub pinned: bool,
}
