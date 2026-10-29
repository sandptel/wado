//! The file manager's two host seams: the desktop's trash, and thumbnails.
//!
//! What a path may reach is never the provider's call: the caller (the file manager's scope)
//! resolves it safely and hands over an open folder and a name in it, so a provider moves exactly
//! what was checked and never resolves a path again.

use std::fs::File;
use std::path::{Path, PathBuf};

use crate::{Fut, Res};

/// A file or folder the caller already reached: its parent folder, open, and its name in it.
pub struct Item<'a> {
    /// The parent folder, opened by the caller.
    pub dir: &'a File,
    /// Where that folder really is (for records such as where an item came from).
    pub dir_path: &'a Path,
    pub name: &'a str,
}

/// The desktop's trash: a delete that can be undone from the desktop's own Trash.
pub trait Trash: Send + Sync {
    /// The folder trashed items are browsed in (the file manager's Trash place).
    fn place(&self) -> PathBuf;
    /// Move `item` to the trash, recording where it came from.
    fn trash(&self, item: Item<'_>) -> Res<()>;
    /// Where `item` — something in the trash — came from. The caller reaches that place safely,
    /// moves the item back, then calls [`Trash::forget`].
    fn origin(&self, item: Item<'_>) -> Res<PathBuf>;
    /// Drop the trash's record of `item` — it was restored.
    fn forget(&self, item: Item<'_>);
}

/// A file to thumbnail, opened by the caller.
pub struct ThumbSource {
    pub file: File,
    /// Where it really is (the cache key).
    pub real: PathBuf,
    /// Modified, in seconds since the epoch — a cached thumbnail older than this is stale.
    pub mtime: u64,
}

/// Thumbnails: the desktop's cache first, else one made now.
pub trait Thumbnailer: Send + Sync {
    /// A small PNG of `src`, or why there is none.
    fn thumb(&self, src: ThumbSource) -> Fut<'_, Res<Vec<u8>>>;
}
