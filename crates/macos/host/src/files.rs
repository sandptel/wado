//! The Mac's Trash and Quick Look thumbnails for the file manager.
//!
//! Trashing goes through `NSFileManager`, the same move Finder makes. Where each item came from
//! is kept in wado's own small record (`~/.config/wado/trash-origins`), because the Mac keeps
//! its "Put Back" record where only Finder reads it. Browsing `~/.Trash` itself needs Full Disk
//! Access, which macOS asks for separately.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use objc2_foundation::{NSFileManager, NSString, NSURL};
use wado_platform::files::{Item, ThumbSource, Thumbnailer, Trash};
use wado_platform::{Fut, Res};

/// Thumbnail size, in pixels on the longer side.
pub const THUMB_PX: u32 = 256;

pub struct MacTrash {
    record: PathBuf,
    lock: Mutex<()>,
}

impl MacTrash {
    pub fn new() -> MacTrash {
        let home = std::env::var("HOME").unwrap_or_default();
        MacTrash::at(Path::new(&home).join(".config/wado/trash-origins"))
    }

    pub fn at(record: PathBuf) -> MacTrash {
        MacTrash {
            record,
            lock: Mutex::new(()),
        }
    }

    fn entries(&self) -> Vec<(String, String)> {
        std::fs::read_to_string(&self.record)
            .unwrap_or_default()
            .lines()
            .filter_map(|l| {
                l.split_once('\t')
                    .map(|(a, b)| (a.to_string(), b.to_string()))
            })
            .collect()
    }

    fn save(&self, e: &[(String, String)]) -> Res<()> {
        if let Some(dir) = self.record.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let body: String = e.iter().map(|(a, b)| format!("{a}\t{b}\n")).collect();
        std::fs::write(&self.record, body).map_err(|e| e.to_string())
    }

    fn remember(&self, name: &str, from: &Path) -> Res<()> {
        let _l = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        let mut e = self.entries();
        e.retain(|(n, _)| n != name);
        e.push((name.to_string(), from.to_string_lossy().into_owned()));
        self.save(&e)
    }
}

impl Default for MacTrash {
    fn default() -> Self {
        Self::new()
    }
}

/// Move `path` to the Trash; the name it has there.
pub fn trash_path(path: &Path) -> Res<String> {
    let url = NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()));
    let mut landed = None;
    NSFileManager::defaultManager()
        .trashItemAtURL_resultingItemURL_error(&url, Some(&mut landed))
        .map_err(|e| e.localizedDescription().to_string())?;
    let landed = landed
        .and_then(|u| u.lastPathComponent())
        .map(|n| n.to_string());
    Ok(landed.unwrap_or_else(|| {
        path.file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    }))
}

impl Trash for MacTrash {
    fn place(&self) -> PathBuf {
        PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".Trash")
    }

    fn trash(&self, item: Item<'_>) -> Res<()> {
        let from = item.dir_path.join(item.name);
        let name = trash_path(&from)?;
        self.remember(&name, &from)
    }

    fn origin(&self, item: Item<'_>) -> Res<PathBuf> {
        self.entries()
            .into_iter()
            .find(|(n, _)| n == item.name)
            .map(|(_, p)| PathBuf::from(p))
            .ok_or_else(|| {
                "this Mac does not say where that came from — put it back in Finder".into()
            })
    }

    fn forget(&self, item: Item<'_>) {
        let _l = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        let mut e = self.entries();
        e.retain(|(n, _)| n != item.name);
        let _ = self.save(&e);
    }
}

/// Thumbnails from Quick Look (`qlmanage`), as PNG.
pub struct MacThumbs;

impl Thumbnailer for MacThumbs {
    fn thumb(&self, src: ThumbSource) -> Fut<'_, Res<Vec<u8>>> {
        Box::pin(async move {
            // ponytail: no cache — Quick Look keeps its own; add one if listings feel slow.
            let out = std::env::temp_dir().join(format!(
                "wado-thumb-{}-{}",
                std::process::id(),
                src.mtime
            ));
            std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
            let size = THUMB_PX.to_string();
            let real = src.real.to_string_lossy().into_owned();
            let o = out.to_string_lossy().into_owned();
            let r = crate::cmd::run(
                "/usr/bin/qlmanage",
                &["-t", "-s", &size, "-o", &o, &real],
                Duration::from_secs(10),
            )
            .await;
            let name = src
                .real
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let png = std::fs::read(out.join(format!("{name}.png")));
            let _ = std::fs::remove_dir_all(&out);
            r?;
            png.map_err(|_| "Quick Look has no picture for this file".into())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("wado-files-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn trash_moves_it_and_remembers_where_from() {
        let d = tmp("trash");
        let f = d.join(format!("wado-test-{}.txt", std::process::id()));
        std::fs::write(&f, "x").unwrap();
        let t = MacTrash::at(d.join("origins"));
        let dir = std::fs::File::open(&d).unwrap();
        let name = f.file_name().unwrap().to_string_lossy().into_owned();
        t.trash(Item {
            dir: &dir,
            dir_path: &d,
            name: &name,
        })
        .unwrap();
        assert!(!f.exists(), "moved");
        let (landed, from) = t.entries().pop().unwrap();
        assert_eq!(PathBuf::from(&from), f);
        assert_eq!(
            t.origin(Item {
                dir: &dir,
                dir_path: &t.place(),
                name: &landed
            })
            .unwrap(),
            f
        );
        t.forget(Item {
            dir: &dir,
            dir_path: &t.place(),
            name: &landed,
        });
        assert!(t.entries().is_empty());
        // Leave the Trash as it was.
        let _ = std::fs::remove_file(t.place().join(&landed));
    }

    #[tokio::test]
    async fn thumb_of_a_text_file_is_png() {
        let d = tmp("thumb");
        let f = d.join("note.txt");
        std::fs::write(&f, "hello\nworld\n").unwrap();
        let png = MacThumbs
            .thumb(ThumbSource {
                file: std::fs::File::open(&f).unwrap(),
                real: f.clone(),
                mtime: 1,
            })
            .await
            .unwrap();
        assert_eq!(&png[..4], b"\x89PNG");
    }
}
