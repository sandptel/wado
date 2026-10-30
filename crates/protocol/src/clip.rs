//! The computer's clipboard on the wire: its history as the Clipboard surface shows it, and the
//! limits every side holds a copy to.
//!
//! **Sharing** (`server::clip`): what is copied on the computer — its desktop, or an app in a
//! wado session — reaches a device that asked for it (`RelayMsg::ClipWatch { live: true }`) as
//! `RelayMsg::ClipNow`; a device's copy reaches the computer as `RelayMsg::ClipPush`. Each copy is
//! named by its [`hash`](ClipEntry) — the first 16 bytes of SHA-256 of its bytes, as lowercase
//! hex — so a value that comes back the way it went is recognised and stops there.
//!
//! **Secrets never cross.** A copy its source marked as a password (`x-kde-passwordManagerHint`
//! and its kin on the computer, a sensitive copy on the phone) is neither sent on nor kept.

use serde::{Deserialize, Serialize};

/// The largest text that is shared: 1 MiB of UTF-8.
pub const MAX_TEXT: usize = 1 << 20;
/// The largest picture that is shared, either way: 8 MiB of PNG or JPEG. As a `data:` URI on the
/// relay link that is under 11 MiB — inside every frame limit on the way.
pub const MAX_IMAGE: usize = 8 << 20;
/// The picture types shared both ways — what a browser's clipboard can hold (PNG), and what a
/// phone camera makes (JPEG).
pub const IMAGE_TYPES: &[&str] = &["image/png", "image/jpeg"];

/// May `mime` with `len` bytes be shared? Text up to [`MAX_TEXT`]; a PNG or JPEG up to
/// [`MAX_IMAGE`]; nothing else.
pub fn shareable(mime: &str, len: usize) -> Result<(), String> {
    if len == 0 {
        return Err("there is nothing on the clipboard".into());
    }
    if mime.starts_with("text/") {
        return if len <= MAX_TEXT {
            Ok(())
        } else {
            Err(format!(
                "that text is too long to share ({} KiB; the most is {} KiB)",
                len >> 10,
                MAX_TEXT >> 10
            ))
        };
    }
    if IMAGE_TYPES.contains(&mime) {
        return if len <= MAX_IMAGE {
            Ok(())
        } else {
            Err(format!(
                "that picture is too big to share ({} MiB; the most is {} MiB)",
                len >> 20,
                MAX_IMAGE >> 20
            ))
        };
    }
    Err(format!("{mime} is not shared — only text, PNG and JPEG"))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClipKind {
    Text,
    Link,
    Image,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClipEntry {
    /// The history's id (cliphist's, or `r<n>` for wado's own); stable while the entry is in it.
    pub id: String,
    pub kind: ClipKind,
    /// The text, trimmed for a tile; for an image, its description (`png 1920x1080 · 154 KiB`).
    pub preview: String,
    #[serde(default)]
    pub pinned: bool,
    /// When it was copied, in unix milliseconds; `0` when the history does not say (cliphist
    /// keeps no times, so only a copy this daemon saw has one).
    #[serde(default)]
    pub at_ms: u64,
    /// Where it was copied: `desktop`, `session`, or the name of the device that sent it; empty
    /// when the history does not say.
    #[serde(default)]
    pub from: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caps_hold_both_ways() {
        assert!(shareable("text/plain", 5).is_ok());
        assert!(shareable("text/plain;charset=utf-8", MAX_TEXT).is_ok());
        assert!(shareable("text/plain", MAX_TEXT + 1).is_err());
        assert!(shareable("image/png", MAX_IMAGE).is_ok());
        assert!(shareable("image/jpeg", MAX_IMAGE + 1).is_err());
        assert!(shareable("image/gif", 10).is_err(), "only PNG and JPEG");
        assert!(shareable("application/x-sh", 10).is_err());
        assert!(shareable("text/plain", 0).is_err(), "nothing is not a copy");
    }

    #[test]
    fn an_older_entry_still_reads() {
        let e: ClipEntry =
            serde_json::from_str(r#"{"id":"7","kind":"text","preview":"hi"}"#).unwrap();
        assert_eq!((e.at_ms, e.from.as_str(), e.pinned), (0, "", false));
    }
}
