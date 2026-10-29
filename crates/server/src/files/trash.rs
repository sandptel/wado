//! Delete means the desktop's trash (`wado_platform::files::Trash`, the freedesktop Trash on
//! Linux): never a hard delete from a phone. What a path may reach is the scope's to decide; the
//! trash only ever gets a folder the scope opened and a name in it.

use std::path::PathBuf;

use wado_platform::files::Item;

use super::scope::{Refusal, Scope, say};

/// Where trashed items are browsed, when this computer has a trash.
pub fn place() -> Option<PathBuf> {
    crate::platform::get().trash().ok().map(|t| t.place())
}

/// Move `path` to the trash.
pub fn trash(sc: &Scope, path: &str) -> Result<(), Refusal> {
    let bin = crate::platform::get().trash().map_err(|a| a.why)?;
    let (dir, name) = sc.parent(path)?;
    bin.trash(Item {
        dir: &dir.file,
        dir_path: &dir.real,
        name: &name,
    })
}

/// Put a trashed item back where it came from — under another name if that one is taken. The
/// original location goes through the scope like any destination.
pub fn restore(sc: &Scope, path: &str) -> Result<String, Refusal> {
    let bin = crate::platform::get().trash().map_err(|a| a.why)?;
    let (files, name) = sc.parent(path)?;
    let item = || Item {
        dir: &files.file,
        dir_path: &files.real,
        name: &name,
    };
    let orig = bin.origin(item())?;
    let orig = orig.to_string_lossy();
    let (back_dir, back_name) = orig.rsplit_once('/').ok_or("bad record")?;
    let to = sc.dir(if back_dir.is_empty() { "/" } else { back_dir })?;
    let target = to.free_name(back_name);
    files.rename_at(&name, &to, &target, false).map_err(say)?;
    bin.forget(item());
    Ok(format!("{back_dir}/{target}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use wado_config::schema::files::Files;

    #[test]
    fn trashing_moves_it_and_records_where_from() {
        let (t, sc) = crate::files::scope::tests::sandbox("trash");
        // The test's own trash, on the same filesystem as the sandbox.
        // SAFETY: only this test reads XDG_DATA_HOME in this process.
        unsafe { std::env::set_var("XDG_DATA_HOME", t.join("data")) };
        std::fs::create_dir_all(t.join("data")).unwrap();
        let a = t.join("root/docs/a.txt");
        trash(&sc, &a.to_string_lossy()).unwrap();
        assert!(!a.exists());
        assert!(t.join("data/Trash/files/a.txt").exists());
        let info = std::fs::read_to_string(t.join("data/Trash/info/a.txt.trashinfo")).unwrap();
        assert!(info.contains(&format!("Path={}", a.display())), "{info}");
        std::fs::write(&a, "again").unwrap();
        trash(&sc, &a.to_string_lossy()).unwrap();
        assert!(
            t.join("data/Trash/files/a.txt.1").exists(),
            "a second one does not clobber"
        );
        assert!(trash(&sc, &t.join("root/secret").to_string_lossy()).is_err());
        // Restore puts it back; the second one, its name now taken, comes back renamed.
        let a_s = a.to_string_lossy().into_owned();
        let trashed = |n: &str| {
            t.join("data/Trash/files")
                .join(n)
                .to_string_lossy()
                .into_owned()
        };
        let cfg = Files {
            root: vec![t.to_string_lossy().into()],
            ..Files::default()
        };
        let wide = Scope::new(&cfg);
        assert_eq!(restore(&wide, &trashed("a.txt")).unwrap(), a_s);
        assert!(a.exists());
        assert!(
            restore(&wide, &trashed("a.txt.1"))
                .unwrap()
                .ends_with("a (1).txt")
        );
    }
}
