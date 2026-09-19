//! Which file on this machine is the icon called "firefox", and how good a fit is it.
//!
//! `Icon=` is usually a bare name, not a path, and resolving it properly means the XDG
//! icon-theme lookup: the user's theme, its `index.theme` inheritance chain, per-context
//! subdirectories, size buckets and `Threshold`/`Scaled` rules. None of that changes *which
//! picture a person recognises* at 64 px, so this does the lazy equivalent — index every icon
//! file once, keep the best-sized candidate per name — and skips themes entirely.
//!
//! ponytail: one cached directory walk, no theme awareness. The visible ceiling is that a
//! machine with several themes installed may serve a Papirus icon where the desktop shows
//! Adwaita. Reading `index.theme` and honouring the GTK theme setting is the upgrade path,
//! and it is only worth it if someone actually complains about the artwork.

use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::OnceLock,
};

/// The size the client draws tiles at. Candidates are scored by distance from it.
const TARGET_PX: u32 = 64;

/// Never walk into a size bucket larger than this — a 512×512 PNG is never the right answer
/// for a 64 px tile, and those buckets are where the file count lives.
const MAX_PX: u32 = 256;

/// An icon file we could serve, and how well it fits.
struct Candidate {
    path: PathBuf,
    /// Lower is better.
    score: u32,
}

/// The best file for an icon name, or `None` when nothing is installed under it.
pub fn lookup(stem: &str) -> Option<PathBuf> {
    index().get(stem).cloned()
}

/// Every icon file on this machine, keyed by file stem, best candidate wins.
///
/// Built once: the walk is the expensive part and the installed icon set does not change
/// while the daemon runs.
fn index() -> &'static HashMap<String, PathBuf> {
    static INDEX: OnceLock<HashMap<String, PathBuf>> = OnceLock::new();
    INDEX.get_or_init(|| {
        let mut best: HashMap<String, Candidate> = HashMap::new();
        for root in roots() {
            walk(&root, 0, &mut best);
        }
        tracing::debug!(icons = best.len(), "indexed icon files");
        best.into_iter().map(|(k, v)| (k, v.path)).collect()
    })
}

/// Where icons live, in no particular order — precedence is decided by score, not by position.
fn roots() -> Vec<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let data_home = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| home.as_ref().map(|h| h.join(".local/share")));
    let data_dirs = std::env::var("XDG_DATA_DIRS")
        .unwrap_or_else(|_| "/usr/local/share:/usr/share".to_string());

    let mut roots: Vec<PathBuf> = data_home
        .into_iter()
        .chain(
            data_dirs
                .split(':')
                .filter(|p| !p.is_empty())
                .map(PathBuf::from),
        )
        .flat_map(|d| [d.join("icons"), d.join("pixmaps")])
        .collect();
    // Predates the icon-theme spec and is still where a hand-installed app drops its PNG.
    roots.extend(home.map(|h| h.join(".icons")));
    roots
}

/// Recursive descent, remembering the best-scoring file for each stem.
fn walk(dir: &Path, depth: u32, best: &mut HashMap<String, Candidate>) {
    // theme / size / context / file is four levels; one spare for oddly nested packages.
    if depth > 5 {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if entry.file_type().is_ok_and(|t| t.is_dir()) {
            // Skipping the oversized buckets is most of what keeps this walk cheap: on a
            // machine with a full theme installed they hold the bulk of the files.
            if bucket_px(name).is_some_and(|px| px > MAX_PX) {
                continue;
            }
            walk(&path, depth + 1, best);
            continue;
        }
        let Some(score) = file_score(&path) else {
            continue;
        };
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let better = best.get(stem).is_none_or(|c| score < c.score);
        if better {
            best.insert(stem.to_string(), Candidate { path, score });
        }
    }
}

/// How well this file fits a 64 px tile, or `None` if it is not a drawable format.
///
/// The size comes from an *ancestor directory*, which is how the icon-theme layout records it.
/// Not the immediate parent: the bucket sits above the context directory
/// (`hicolor/48x48/apps/firefox.png`), and some themes invert the two
/// (`Papirus/apps/48/firefox.svg`). A file under no such directory — a pixmap, say — gets a
/// middling score: usable, but beaten by a correctly sized theme icon.
fn file_score(path: &Path) -> Option<u32> {
    // xpm is in `pixmaps` everywhere and no browser renders it.
    let ext_penalty = match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
        "png" => 0,
        // Scales to any tile size, but is also where the multi-hundred-KB files are.
        "svg" => 1,
        _ => return None,
    };
    let px = path
        .ancestors()
        .skip(1)
        .take(3)
        .filter_map(|p| p.file_name()?.to_str())
        .find_map(bucket_px);
    let size_score = match px {
        Some(px) => px.abs_diff(TARGET_PX),
        // "scalable" and everything unbucketed: fine, but not proof of a good fit.
        None => 24,
    };
    Some(size_score * 2 + ext_penalty)
}

/// `"48x48"` → 48, and `"48"` → 48 for the themes that name the bucket by one number.
/// `None` for `scalable`, `apps`, a theme name, anything else.
fn bucket_px(dir: &str) -> Option<u32> {
    match dir.split_once('x') {
        Some((w, h)) => {
            let w: u32 = w.parse().ok()?;
            (w == h.parse::<u32>().ok()?).then_some(w)
        }
        None => dir.parse().ok(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_closer_size_bucket_beats_a_further_one() {
        // The bucket is the grandparent here, which is the layout every theme actually uses —
        // reading only the immediate parent scored every icon identically.
        let at64 = file_score(Path::new("/i/hicolor/64x64/apps/x.png")).unwrap();
        let at48 = file_score(Path::new("/i/hicolor/48x48/apps/x.png")).unwrap();
        let at16 = file_score(Path::new("/i/hicolor/16x16/apps/x.png")).unwrap();
        let svg = file_score(Path::new("/i/hicolor/scalable/apps/x.svg")).unwrap();
        assert!(at64 < at48 && at48 < at16, "{at64} {at48} {at16}");
        // A 16px icon on a 64px tile is a blurry mess; scalable wins that one.
        assert!(svg < at16, "{svg} {at16}");
        // An exact hit still beats scalable.
        assert!(at64 < svg, "{at64} {svg}");
    }

    #[test]
    fn only_formats_a_browser_can_draw_are_offered() {
        assert!(file_score(Path::new("/usr/share/pixmaps/x.xpm")).is_none());
        assert!(file_score(Path::new("/usr/share/pixmaps/x")).is_none());
        assert!(file_score(Path::new("/usr/share/pixmaps/x.png")).is_some());
    }

    #[test]
    fn size_buckets_are_read_but_other_directories_are_not() {
        assert_eq!(bucket_px("48x48"), Some(48));
        assert_eq!(bucket_px("512x512"), Some(512));
        assert_eq!(bucket_px("scalable"), None);
        assert_eq!(bucket_px("apps"), None);
        assert_eq!(bucket_px("16x16@2x"), None);
        // Papirus and friends name the bucket with one number.
        assert_eq!(bucket_px("48"), Some(48));
    }
}
