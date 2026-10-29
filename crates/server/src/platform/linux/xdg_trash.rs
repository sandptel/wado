//! The desktop's trash on Linux ([`Trash`]): the freedesktop Trash (the spec Nautilus and every
//! desktop follow). The item is moved into `Trash/files/` and a `.trashinfo` beside it records
//! where it came from, so it can be restored from the desktop's own Trash. A phone never
//! hard-deletes.
//!
//! On the home filesystem that is `$XDG_DATA_HOME/Trash`; elsewhere the spec's per-volume
//! `$topdir/.Trash-$uid`, so nothing is copied across filesystems to be deleted.

use std::io::Write;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use percent_encoding::{AsciiSet, CONTROLS, utf8_percent_encode};
use wado_platform::Res;
use wado_platform::files::{Item, Trash};

use crate::files::scope::{Opened, say};

/// The freedesktop Trash.
pub struct XdgTrash;

impl Trash for XdgTrash {
    fn place(&self) -> PathBuf {
        home_trash().join("files")
    }

    fn trash(&self, item: Item<'_>) -> Res<()> {
        trash(item)
    }

    fn origin(&self, item: Item<'_>) -> Res<PathBuf> {
        origin(item)
    }

    fn forget(&self, item: Item<'_>) {
        if let Some(bin) = item.dir_path.parent() {
            let _ = std::fs::remove_file(bin.join("info").join(format!("{}.trashinfo", item.name)));
        }
    }
}

/// The folder `item` is in, as the scope's handle — the moves below go through its descriptor.
fn opened(item: &Item<'_>) -> Result<Opened, String> {
    Ok(Opened {
        file: item.dir.try_clone().map_err(say)?,
        real: item.dir_path.to_path_buf(),
    })
}

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

pub fn home_trash() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| crate::files::scope::home().join(".local/share"))
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

fn trash(item: Item<'_>) -> Res<()> {
    let dir = opened(&item)?;
    let name = item.name.to_string();
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
    let t = crate::files::local(crate::files::now_s());
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

/// Where a trashed item came from, by its `.trashinfo` — a relative `Path=` is relative to the
/// volume holding this Trash.
fn origin(item: Item<'_>) -> Res<PathBuf> {
    if item.dir_path.file_name().and_then(|n| n.to_str()) != Some("files") {
        return Err("that is not in a Trash".into());
    }
    let bin = item.dir_path.parent().ok_or("that is not in a Trash")?;
    let info_path = bin.join("info").join(format!("{}.trashinfo", item.name));
    let info =
        std::fs::read_to_string(&info_path).map_err(|_| "no record of where this came from")?;
    let orig = info
        .lines()
        .find_map(|l| l.strip_prefix("Path="))
        .ok_or("no record of where this came from")?;
    let orig = percent_encoding::percent_decode_str(orig)
        .decode_utf8_lossy()
        .into_owned();
    Ok(if orig.starts_with('/') {
        PathBuf::from(orig)
    } else {
        bin.parent().unwrap_or(bin).join(orig)
    })
}
