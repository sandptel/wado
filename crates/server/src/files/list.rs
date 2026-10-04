//! A folder's entries.

use serde_json::{Value, json};

use super::scope::{Refusal, Scope, say};

pub fn list(sc: &Scope, path: &str) -> Result<Value, Refusal> {
    let dir = sc.dir(path)?;
    let mut entries = Vec::new();
    for e in std::fs::read_dir(dir.here()).map_err(say)?.flatten() {
        // ponytail: a name that is not UTF-8 is skipped; byte-exact names are the upgrade path.
        let Ok(name) = e.file_name().into_string() else {
            continue;
        };
        if !sc.shows(&dir.real, &name) {
            continue;
        }
        let Ok(md) = e.metadata() else { continue };
        let link = md.file_type().is_symlink();
        // A symlink is described by what it points at — but only if that is inside the scope;
        // one leading out is listed as a bare link, with nothing about its target.
        let md = if link {
            match sc.open(
                &format!("{}/{name}", path.trim_end_matches('/')),
                libc::O_PATH,
            ) {
                Ok(o) => o.file.metadata().unwrap_or(md),
                Err(_) => md,
            }
        } else {
            md
        };
        entries.push(json!({
            "name": name,
            "dir": md.is_dir(),
            "size": if md.is_file() { md.len() } else { 0 },
            "mtime": super::secs(md.modified()),
            "link": link,
        }));
    }
    Ok(json!({ "path": path, "entries": entries }))
}
