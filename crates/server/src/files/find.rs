//! Search: names containing a query, or files of a kind, under a folder or every root —
//! for the search box and the home page's categories (Photos, Videos, …).
//!
//! A walk, bounded twice: by the folders it may open and by the matches it keeps, so a search
//! of `/` answers in a second with "here is what I found so far" rather than never. Every folder
//! is opened through the scope; denied trees are skipped, and so are dot-folders unless `hidden`.
//!
//! ponytail: a plain walk per search. An index (or `locate`/`tracker` when the host has one) is
//! the upgrade if people search very large trees often.

use serde_json::{Value, json};

use super::scope::Scope;

const MAX_DIRS: usize = 4000;
const MAX_HITS: usize = 300;
const MAX_DEPTH: usize = 14;

fn kind_of(name: &str) -> &'static str {
    let ext = name
        .rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "avif" | "bmp" | "heic" | "tif" | "tiff"
        | "svg" | "raw" | "cr2" | "nef" => "image",
        "mp4" | "m4v" | "mkv" | "webm" | "mov" | "avi" | "wmv" | "flv" | "mpg" | "mpeg" | "ts"
        | "m2ts" | "3gp" | "ogv" => "video",
        "mp3" | "m4a" | "aac" | "ogg" | "opus" | "flac" | "wav" | "wma" | "aiff" | "ape" => "audio",
        "pdf" | "doc" | "docx" | "odt" | "rtf" | "txt" | "md" | "xls" | "xlsx" | "ods" | "csv"
        | "ppt" | "pptx" | "odp" | "epub" => "doc",
        "zip" | "tar" | "gz" | "tgz" | "xz" | "bz2" | "zst" | "7z" | "rar" | "iso" => "archive",
        _ => "",
    }
}

pub fn find(sc: &Scope, path: &str, query: &str, kind: &str) -> Value {
    let q = query.to_lowercase();
    let starts: Vec<String> = if path.is_empty() {
        sc.roots()
            .iter()
            .map(|r| r.to_string_lossy().into_owned())
            .collect()
    } else {
        vec![path.to_string()]
    };
    let mut hits: Vec<(u64, Value)> = Vec::new();
    let mut dirs = 0usize;
    let mut queue: std::collections::VecDeque<(String, usize)> =
        starts.into_iter().map(|p| (p, 0)).collect();
    let mut seen = std::collections::HashSet::new();
    while let Some((dir, depth)) = queue.pop_front() {
        if dirs >= MAX_DIRS || hits.len() >= MAX_HITS * 4 {
            break;
        }
        let Ok(d) = sc.dir(&dir) else { continue };
        // Roots can nest (`~` inside `/`): each real folder once.
        if !seen.insert(d.real.clone()) {
            continue;
        }
        dirs += 1;
        for e in std::fs::read_dir(d.here()).into_iter().flatten().flatten() {
            let Ok(name) = e.file_name().into_string() else {
                continue;
            };
            if !sc.shows(&d.real, &name) {
                continue;
            }
            let Ok(md) = e.metadata() else { continue };
            let child = format!("{}/{name}", dir.trim_end_matches('/'));
            if md.is_dir() {
                if depth < MAX_DEPTH && !md.file_type().is_symlink() {
                    queue.push_back((child.clone(), depth + 1));
                }
            }
            let name_ok = q.is_empty() || name.to_lowercase().contains(&q);
            let kind_ok = kind.is_empty() || (md.is_file() && kind_of(&name) == kind);
            if name_ok && kind_ok && (md.is_file() || (md.is_dir() && kind.is_empty())) {
                let mtime = super::secs(md.modified());
                hits.push((mtime, json!({ "name": name, "path": child, "dir": md.is_dir(), "size": if md.is_file() { md.len() } else { 0 }, "mtime": mtime })));
            }
        }
    }
    hits.sort_by(|a, b| b.0.cmp(&a.0));
    let partial = dirs >= MAX_DIRS || hits.len() > MAX_HITS;
    let entries: Vec<Value> = hits.into_iter().take(MAX_HITS).map(|(_, v)| v).collect();
    json!({ "entries": entries, "partial": partial })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_by_name_and_kind_but_never_in_denied_trees() {
        let (t, sc) = crate::files::scope::tests::sandbox("find");
        let r = t.join("root");
        std::fs::write(r.join("docs/photo.JPG"), "x").unwrap();
        std::fs::write(r.join("secret/photo.png"), "x").unwrap();
        let root = r.to_string_lossy().into_owned();
        let names = |v: Value| {
            v["entries"]
                .as_array()
                .unwrap()
                .iter()
                .map(|e| e["name"].as_str().unwrap().to_string())
                .collect::<Vec<_>>()
        };
        assert_eq!(names(find(&sc, &root, "PHOTO", "")), ["photo.JPG"]);
        assert_eq!(names(find(&sc, &root, "", "image")), ["photo.JPG"]);
        assert!(names(find(&sc, &root, "a.txt", "")).contains(&"a.txt".to_string()));
        assert!(
            names(find(&sc, "", "", "image")).len() <= 1,
            "every root, still no denied tree"
        );
    }
}
