//! The desktop's clipboard, and its history.

use std::sync::Arc;

use tokio::sync::watch;
use wado_protocol::ClipEntry;

use crate::{Fut, Res};

/// Clipboard contents: a mime type and the bytes. Shared, so a copy crossing between the session
/// and the desktop is never duplicated. Empty `data` is nothing copied yet.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Clip {
    pub mime: String,
    pub data: Arc<[u8]>,
}

/// What a text selection is offered as, best first.
pub const TEXT: &[&str] = &[
    "text/plain;charset=utf-8",
    "text/plain",
    "UTF8_STRING",
    "TEXT",
    "STRING",
];

impl Clip {
    pub fn is_text(&self) -> bool {
        self.mime.starts_with("text/") || TEXT.contains(&self.mime.as_str())
    }
}

/// The desktop's clipboard.
pub trait Clipboard: Send + Sync {
    /// What is on it now: text if offered, else the first image type.
    fn get(&self) -> Fut<'_, Res<Clip>>;
    fn set(&self, clip: Clip) -> Fut<'_, Res<()>>;
    /// Ticks once per change on the desktop (a change this side made included).
    fn changes(&self) -> watch::Receiver<u64>;
}

/// The desktop's clipboard history, newest first, pinned entries on top. An `id` is the
/// provider's and is checked by it before use.
pub trait ClipHistory: Send + Sync {
    fn list(&self) -> Fut<'_, Res<Vec<ClipEntry>>>;
    fn get(&self, id: String) -> Fut<'_, Res<Clip>>;
    fn delete(&self, id: String) -> Fut<'_, Res<()>>;
    fn pin(&self, id: String, on: bool) -> Fut<'_, Res<()>>;
}
