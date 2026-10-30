//! The folders the new-session picker walks: the subfolders of one folder, home first, hidden
//! ones left out, and the folders Claude was used in lately for one tap.

use std::path::{Path, PathBuf};

use wado_protocol::claude::ClaudeDirs;

const MAX: usize = 400;

fn home() -> PathBuf {
    std::env::var_os("HOME").map_or_else(|| PathBuf::from("/"), PathBuf::from)
}

/// The subfolders of `path` (home when empty), sorted the way a person reads them.
pub fn list(path: &str, recent: Vec<String>) -> ClaudeDirs {
    let at = if path.is_empty() {
        home()
    } else {
        PathBuf::from(path)
    };
    let at = match at.canonicalize() {
        Ok(p) if p.is_dir() => p,
        _ => {
            return ClaudeDirs {
                path: path.into(),
                error: format!("{path} isn't a folder on the computer"),
                recent,
                ..Default::default()
            };
        }
    };
    let mut dirs: Vec<String> = std::fs::read_dir(&at)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| !n.starts_with('.'))
        .collect();
    dirs.sort_by_key(|n| n.to_lowercase());
    dirs.truncate(MAX);
    ClaudeDirs {
        parent: at
            .parent()
            .map(Path::to_string_lossy)
            .map(|p| p.into_owned())
            .unwrap_or_default(),
        path: at.to_string_lossy().into_owned(),
        dirs,
        recent: recent
            .into_iter()
            .filter(|r| Path::new(r).is_dir())
            .take(5)
            .collect(),
        error: String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_folders_only_without_hidden_ones() {
        let dir = std::env::temp_dir().join(format!("wado-dirs-{}", rand::random::<u64>()));
        for d in ["beta", "Alpha", ".git"] {
            std::fs::create_dir_all(dir.join(d)).unwrap();
        }
        std::fs::write(dir.join("file.txt"), "x").unwrap();
        let l = list(
            dir.to_str().unwrap(),
            vec![dir.join("beta").to_string_lossy().into(), "/nope".into()],
        );
        assert_eq!(l.dirs, ["Alpha", "beta"]);
        assert_eq!(
            l.recent.len(),
            1,
            "a recent folder that is gone is left out"
        );
        assert!(!l.parent.is_empty());
        assert!(!list("/definitely/not/here", vec![]).error.is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }
}
