//! Quick access: pinned folders and recent files, the same for every device (Decision Log
//! `2026-10-04`, item 11).
//!
//! Pins are the GTK bookmarks file — the sidebar Nautilus shows, so a pin made on the phone
//! appears on the desktop and back — plus the XDG folders. Recents merge GTK's
//! `recently-used.xbel` with a bounded look at what changed in the pinned folders. Everything
//! goes through the scope: a pin or a recent file outside it is simply not shown.

use std::path::PathBuf;

use percent_encoding::{AsciiSet, CONTROLS, percent_decode_str, utf8_percent_encode};
use serde_json::{Value, json};

use super::scope::{Refusal, Scope, home, say};

const URI: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'#')
    .add(b'%')
    .add(b'<')
    .add(b'>')
    .add(b'?');
/// How many recent files are shown, and how many entries the mtime scan may look at.
const RECENT: usize = 40;
const SCAN_BUDGET: usize = 4000;

fn config_home() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME").map_or_else(|| home().join(".config"), PathBuf::from)
}
fn data_home() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME").map_or_else(|| home().join(".local/share"), PathBuf::from)
}
fn bookmarks_file() -> PathBuf {
    config_home().join("gtk-3.0/bookmarks")
}

fn from_uri(uri: &str) -> Option<String> {
    let p = uri.strip_prefix("file://")?;
    percent_decode_str(p)
        .decode_utf8()
        .ok()
        .map(|s| s.into_owned())
}

/// `(path, label)` of each GTK bookmark.
fn bookmarks() -> Vec<(String, String)> {
    std::fs::read_to_string(bookmarks_file())
        .unwrap_or_default()
        .lines()
        .filter_map(|l| {
            let (uri, label) = l.split_once(' ').unwrap_or((l, ""));
            let path = from_uri(uri.trim())?;
            let label = if label.is_empty() {
                path.rsplit('/').next().unwrap_or(&path).to_string()
            } else {
                label.to_string()
            };
            Some((path, label))
        })
        .collect()
}

/// The XDG user folders that exist: Downloads, Documents, …
fn xdg() -> Vec<(String, String)> {
    let file = std::fs::read_to_string(config_home().join("user-dirs.dirs")).unwrap_or_default();
    let h = home().to_string_lossy().into_owned();
    let mut out: Vec<(String, String)> = file
        .lines()
        .filter_map(|l| {
            let (_, v) = l.split_once('=')?;
            let p = v.trim().trim_matches('"').replace("$HOME", &h);
            (p != h).then(|| (p.clone(), p.rsplit('/').next().unwrap_or("").to_string()))
        })
        .collect();
    if out.is_empty() {
        for d in [
            "Downloads",
            "Documents",
            "Pictures",
            "Music",
            "Videos",
            "Desktop",
        ] {
            out.push((format!("{h}/{d}"), d.into()));
        }
    }
    out
}

/// `(path, mtime)` from `recently-used.xbel`.
fn xbel() -> Vec<String> {
    let text = std::fs::read_to_string(data_home().join("recently-used.xbel")).unwrap_or_default();
    text.split("<bookmark ")
        .skip(1)
        .filter_map(|b| {
            let href = b.split("href=\"").nth(1)?.split('"').next()?;
            from_uri(&href.replace("&amp;", "&"))
        })
        .collect()
}

pub fn quick(sc: &Scope) -> Value {
    let mut pins = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for (path, label, kind) in bookmarks()
        .into_iter()
        .map(|(p, l)| (p, l, "pin"))
        .chain(xdg().into_iter().map(|(p, l)| (p, l, "xdg")))
    {
        if seen.insert(path.clone()) && sc.dir(&path).is_ok() {
            pins.push(json!({ "path": path, "name": label, "kind": kind }));
        }
    }

    // Recents: the xbel list, then the newest files directly in the pinned folders.
    let mut recent: Vec<(String, u64, u64)> = Vec::new();
    let mut add = |sc: &Scope, path: String| {
        if let Ok(o) = sc.open(&path, libc::O_PATH) {
            if let Ok(md) = o.file.metadata() {
                if md.is_file() {
                    recent.push((path, super::secs(md.modified()), md.len()));
                }
            }
        }
    };
    for p in xbel().into_iter().rev().take(RECENT * 2) {
        add(sc, p);
    }
    let mut budget = SCAN_BUDGET;
    for pin in &pins {
        let Some(dir) = pin["path"].as_str() else {
            continue;
        };
        let Ok(d) = sc.dir(dir) else { continue };
        for e in std::fs::read_dir(d.here()).into_iter().flatten().flatten() {
            if budget == 0 {
                break;
            }
            budget -= 1;
            if let Ok(n) = e.file_name().into_string() {
                if sc.shows(&d.real, &n) && e.file_type().is_ok_and(|t| t.is_file()) {
                    add(sc, format!("{dir}/{n}"));
                }
            }
        }
    }
    recent.sort_by(|a, b| b.1.cmp(&a.1));
    recent.dedup_by(|a, b| a.0 == b.0);
    let recent: Vec<_> = recent
        .into_iter()
        .take(RECENT)
        .map(|(path, mtime, size)| {
            let name = path.rsplit('/').next().unwrap_or("").to_string();
            json!({ "path": path, "name": name, "mtime": mtime, "size": size })
        })
        .collect();
    json!({ "pins": pins, "recent": recent })
}

/// Add or remove a GTK bookmark.
pub fn pin(sc: &Scope, path: &str, on: bool) -> Result<(), Refusal> {
    sc.dir(path)?;
    let uri = format!("file://{}", utf8_percent_encode(path, URI));
    let file = bookmarks_file();
    let text = std::fs::read_to_string(&file).unwrap_or_default();
    let mut lines: Vec<&str> = text
        .lines()
        .filter(|l| l.split(' ').next() != Some(uri.as_str()))
        .collect();
    if on {
        lines.push(&uri);
    }
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir).map_err(say)?;
    }
    let mut out = lines.join("\n");
    out.push('\n');
    std::fs::write(&file, out).map_err(say)
}
