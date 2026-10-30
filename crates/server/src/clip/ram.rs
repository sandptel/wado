//! wado's own clipboard history, in memory only: the last [`KEEP`] copies the hub shared, pinned
//! ones kept on top. It is the history on a computer without `cliphist` (a headless daemon, a
//! desktop that never installed it), and where `cliphist` is the history, it lends its entries
//! the time and origin `cliphist` does not keep ([`Ram::annotate`]).
//!
//! Nothing here touches the disk: a restart forgets it, which is the point for a clipboard. A
//! secret never reaches it (the hub stops it first).

use std::collections::VecDeque;
use std::sync::Mutex;

use wado_compositor::clipboard::Clip;
use wado_platform::clip::ClipHistory;
use wado_platform::{Fut, Res};
use wado_protocol::{ClipEntry, ClipKind};

use super::hub::{Copied, cliphist_line};

/// How many unpinned copies are kept.
pub const KEEP: usize = 50;
/// And at most this many bytes of them, all told: fifty screenshots must not take the memory.
pub const BYTES: usize = 96 << 20;
/// A tile's preview, in characters.
const PREVIEW: usize = 280;

struct Item {
    id: String,
    hash: String,
    clip: Clip,
    from: String,
    at_ms: u64,
    pinned: bool,
}

#[derive(Default)]
struct Inner {
    next: u64,
    /// Newest first.
    items: VecDeque<Item>,
}

#[derive(Default)]
pub struct Ram(Mutex<Inner>);

impl Ram {
    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Keep `c`. The same bytes copied again move to the top, keeping their pin.
    pub fn record(&self, c: &Copied) {
        let mut g = self.lock();
        let pinned = match g.items.iter().position(|i| i.hash == c.hash) {
            Some(at) => g.items.remove(at).is_some_and(|i| i.pinned),
            None => false,
        };
        g.next += 1;
        let id = format!("r{}", g.next);
        g.items.push_front(Item {
            id,
            hash: c.hash.clone(),
            clip: c.clip.clone(),
            from: c.from.label(),
            at_ms: c.at_ms,
            pinned,
        });
        // The oldest unpinned go first, past the count or the bytes.
        loop {
            let unpinned = g.items.iter().filter(|i| !i.pinned).count();
            let bytes: usize = g.items.iter().map(|i| i.clip.data.len()).sum();
            if unpinned <= KEEP && bytes <= BYTES {
                break;
            }
            match g.items.iter().rposition(|i| !i.pinned) {
                Some(at) if at > 0 => {
                    g.items.remove(at);
                }
                _ => break,
            }
        }
    }

    /// The history as a tile list: pinned first, then newest first.
    pub fn list_now(&self) -> Vec<ClipEntry> {
        let g = self.lock();
        let mut all: Vec<ClipEntry> = g.items.iter().map(entry).collect();
        all.sort_by_key(|e| !e.pinned); // stable
        all
    }

    /// Lend `cliphist`'s entries the time and origin of the same copy here, matched by the line
    /// `cliphist` shows for it.
    pub fn annotate(&self, entries: &mut [ClipEntry]) {
        let g = self.lock();
        for e in entries.iter_mut().filter(|e| e.kind != ClipKind::Image) {
            if let Some(i) = g
                .items
                .iter()
                .filter(|i| i.clip.is_text())
                .find(|i| cliphist_line(&String::from_utf8_lossy(&i.clip.data)) == e.preview)
            {
                e.at_ms = i.at_ms;
                e.from.clone_from(&i.from);
            }
        }
    }
}

fn entry(i: &Item) -> ClipEntry {
    let (kind, preview) = if i.clip.is_text() {
        let text = String::from_utf8_lossy(&i.clip.data);
        let t = text.trim();
        let link = (t.starts_with("https://") || t.starts_with("http://"))
            && !t.contains(char::is_whitespace);
        let kind = if link { ClipKind::Link } else { ClipKind::Text };
        (kind, t.chars().take(PREVIEW).collect())
    } else {
        (ClipKind::Image, image_label(&i.clip))
    };
    ClipEntry {
        id: i.id.clone(),
        kind,
        preview,
        pinned: i.pinned,
        at_ms: i.at_ms,
        from: i.from.clone(),
    }
}

/// `png 1920x1080 · 154 KiB`, as `cliphist`'s entries read.
fn image_label(c: &Clip) -> String {
    let ty = c.mime.trim_start_matches("image/");
    let size = size(c.data.len());
    match dims(&c.data) {
        Some((w, h)) => format!("{ty} {w}x{h} · {size}"),
        None => format!("{ty} · {size}"),
    }
}

fn size(n: usize) -> String {
    match n {
        n if n >= 1 << 20 => format!("{} MiB", n >> 20),
        n if n >= 1 << 10 => format!("{} KiB", n >> 10),
        n => format!("{n} B"),
    }
}

/// A PNG's or JPEG's width and height, from its header.
fn dims(b: &[u8]) -> Option<(u32, u32)> {
    let be32 = |o: usize| Some(u32::from_be_bytes(b.get(o..o + 4)?.try_into().ok()?));
    let be16 = |o: usize| Some(u16::from_be_bytes(b.get(o..o + 2)?.try_into().ok()?) as u32);
    if b.starts_with(b"\x89PNG") {
        return Some((be32(16)?, be32(20)?));
    }
    if !b.starts_with(&[0xFF, 0xD8]) {
        return None;
    }
    // Walk the JPEG's segments to its frame header (SOF0–SOF15, less DHT/JPG/DAC).
    let mut o = 2;
    while o + 4 <= b.len() {
        if b[o] != 0xFF {
            return None;
        }
        let marker = b[o + 1];
        let len = be16(o + 2)? as usize;
        if (0xC0..=0xCF).contains(&marker) && ![0xC4, 0xC8, 0xCC].contains(&marker) {
            return Some((be16(o + 7)?, be16(o + 5)?));
        }
        o += 2 + len;
    }
    None
}

fn missing() -> String {
    "that entry is no longer in the history".into()
}

impl ClipHistory for Ram {
    fn list(&self) -> Fut<'_, Res<Vec<ClipEntry>>> {
        Box::pin(async move { Ok(self.list_now()) })
    }

    fn get(&self, id: String) -> Fut<'_, Res<Clip>> {
        Box::pin(async move {
            let g = self.lock();
            g.items
                .iter()
                .find(|i| i.id == id)
                .map(|i| i.clip.clone())
                .ok_or_else(missing)
        })
    }

    fn delete(&self, id: String) -> Fut<'_, Res<()>> {
        Box::pin(async move {
            let mut g = self.lock();
            let at = g
                .items
                .iter()
                .position(|i| i.id == id)
                .ok_or_else(missing)?;
            g.items.remove(at);
            Ok(())
        })
    }

    fn pin(&self, id: String, on: bool) -> Fut<'_, Res<()>> {
        Box::pin(async move {
            let mut g = self.lock();
            let i = g
                .items
                .iter_mut()
                .find(|i| i.id == id)
                .ok_or_else(missing)?;
            i.pinned = on;
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clip::hub::{Origin, hash};

    fn copied(s: &[u8], mime: &str, at: u64) -> Copied {
        Copied {
            hash: hash(s),
            clip: Clip {
                mime: mime.into(),
                data: s.into(),
                secret: false,
            },
            from: Origin::Session,
            at_ms: at,
        }
    }

    #[tokio::test]
    async fn keeps_the_last_fifty_with_pins_on_top() {
        let r = Ram::default();
        for n in 0..60u64 {
            r.record(&copied(format!("copy {n}").as_bytes(), "text/plain", n));
        }
        let all = r.list_now();
        assert_eq!(all.len(), KEEP);
        assert_eq!(all[0].preview, "copy 59", "newest first");
        assert_eq!(all[0].from, "session");
        assert_eq!(all[0].at_ms, 59);
        // Pin an old one: it goes on top, and survives fifty more.
        let old = all.last().unwrap().id.clone();
        r.pin(old.clone(), true).await.unwrap();
        for n in 60..120u64 {
            r.record(&copied(format!("copy {n}").as_bytes(), "text/plain", n));
        }
        let all = r.list_now();
        assert_eq!(all[0].id, old);
        assert!(all[0].pinned);
        assert_eq!(all.len(), KEEP + 1);
        // Copied again: to the top of its group, with its pin.
        r.record(&copied(b"copy 10", "text/plain", 999));
        assert_eq!(r.list_now()[0].preview, "copy 10");
        assert!(r.list_now()[0].pinned);
        r.delete(old).await.ok();
        let first = r.list_now()[0].clone();
        assert_eq!(
            r.get(first.id.clone()).await.unwrap().data.as_ref(),
            b"copy 10"
        );
        r.delete(first.id.clone()).await.unwrap();
        assert!(r.get(first.id).await.is_err());
    }

    #[test]
    fn bytes_are_capped_too() {
        let r = Ram::default();
        let big = vec![0u8; BYTES / 3 + 1];
        for n in 0..5u8 {
            let mut b = big.clone();
            b[0] = n;
            r.record(&copied(&b, "image/png", n.into()));
        }
        assert_eq!(
            r.list_now().len(),
            2,
            "two thirds-plus fit, a third does not"
        );
    }

    #[test]
    fn kinds_labels_and_annotation() {
        let r = Ram::default();
        r.record(&copied(b"https://wado.dev/x", "text/plain", 5));
        // A 2x3 PNG header.
        let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        png.extend_from_slice(&2u32.to_be_bytes());
        png.extend_from_slice(&3u32.to_be_bytes());
        r.record(&copied(&png, "image/png", 6));
        let all = r.list_now();
        assert_eq!(all[0].kind, ClipKind::Image);
        assert_eq!(all[0].preview, "png 2x3 · 24 B");
        assert_eq!(all[1].kind, ClipKind::Link);

        let mut from_cliphist = vec![ClipEntry {
            id: "41".into(),
            kind: ClipKind::Link,
            preview: "https://wado.dev/x".into(),
            pinned: false,
            at_ms: 0,
            from: String::new(),
        }];
        r.annotate(&mut from_cliphist);
        assert_eq!(from_cliphist[0].at_ms, 5);
        assert_eq!(from_cliphist[0].from, "session");
    }

    #[test]
    fn jpeg_dimensions() {
        // SOI, an APP0 of 4 bytes, then SOF0: len 11, precision 8, height 480, width 640.
        let jpg = [
            0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x04, 0x00, 0x00, 0xFF, 0xC0, 0x00, 0x0B, 0x08, 0x01,
            0xE0, 0x02, 0x80,
        ];
        assert_eq!(dims(&jpg), Some((640, 480)));
        assert_eq!(dims(b"GIF89a"), None);
    }
}
