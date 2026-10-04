//! Delete means the freedesktop Trash (the spec Nautilus and every desktop follow): the item is
//! moved into `Trash/files/` and a `.trashinfo` beside it records where it came from, so it can
//! be restored from the desktop's own Trash. A phone never hard-deletes.
//!
//! On the home filesystem that is `$XDG_DATA_HOME/Trash`; elsewhere the spec's per-volume
//! `$topdir/.Trash-$uid`, so nothing is copied across filesystems to be deleted.

use std::io::Write;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use percent_encoding::{AsciiSet, CONTROLS, utf8_percent_encode};

use super::scope::{Opened, Refusal, Scope, say};

/// What the spec says to escape in `Path=`: everything but unreserved characters and `/`.
const URI: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'#')
    .add(b'%')
    .add(b'<')
    .add(b'>')
    .add(b'?')
    .add(b'[')
    .add(b'\\')
    .add(b']')
    .add(b'^')
    .add(b'`')
    .add(b'{')
    .add(b'|')
    .add(b'}');

fn home_trash() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| super::scope::home().join(".local/share"))
        .join("Trash")
}

/// The mount point holding `p`: the last ancestor on the same device.
fn topdir(p: &Path, dev: u64) -> PathBuf {
    let mut top = p.to_path_buf();
    for a in p.ancestors().skip(1) {
        match std::fs::metadata(a) {
            Ok(m) if m.dev() == dev => top = a.to_path_buf(),
            _ => break,
        }
    }
    top
}

/// The trash for something on device `dev` at `real`.
fn trash_for(real: &Path, dev: u64) -> PathBuf {
    let home = home_trash();
    let home_dev = std::fs::metadata(home.parent().unwrap_or(&home)).map(|m| m.dev());
    if home_dev.is_ok_and(|d| d == dev) {
        home
    } else {
        // SAFETY: getuid cannot fail.
        topdir(real, dev).join(format!(".Trash-{}", unsafe { libc::getuid() }))
    }
}

pub fn trash(sc: &Scope, path: &str) -> Result<(), Refusal> {
    let (dir, name) = sc.parent(path)?;
    let real = dir.real.join(&name);
    let md = std::fs::symlink_metadata(dir.at(&name)).map_err(say)?;
    let bin = trash_for(&real, md.dev());
    for sub in ["files", "info"] {
        std::fs::create_dir_all(bin.join(sub)).map_err(say)?;
    }
    let files = Opened {
        file: std::fs::File::open(bin.join("files")).map_err(say)?,
        real: bin.join("files"),
    };
    // Reserve a name by creating its info file exclusively, as the spec says to.
    let mut n = 0;
    let (slot, mut info) = loop {
        let slot = if n == 0 {
            name.clone()
        } else {
            format!("{name}.{n}")
        };
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(bin.join("info").join(format!("{slot}.trashinfo")))
        {
            Ok(f) => break (slot, f),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => n += 1,
            Err(e) => return Err(say(e)),
        }
    };
    let t = super::local(super::now_s());
    let when = format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
        t.tm_year + 1900,
        t.tm_mon + 1,
        t.tm_mday,
        t.tm_hour,
        t.tm_min,
        t.tm_sec
    );
    let shown = real.to_string_lossy();
    let r = writeln!(
        info,
        "[Trash Info]\nPath={}\nDeletionDate={when}",
        utf8_percent_encode(&shown, URI)
    )
    .map_err(say)
    .and_then(|_| dir.rename_at(&name, &files, &slot, false).map_err(say));
    if r.is_err() {
        let _ = std::fs::remove_file(bin.join("info").join(format!("{slot}.trashinfo")));
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;

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
    }
}
