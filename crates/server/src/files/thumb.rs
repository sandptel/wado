//! Thumbnails: the host's freedesktop thumbnail cache first (what Nautilus already made), else
//! one made now — for images only, by `gdk-pixbuf-thumbnailer` when the host has it — and kept
//! in wado's own cache rather than written into the shared one.
//!
//! The thumbnailer is handed `/proc/<pid>/fd/<n>` of the file this daemon opened through the
//! scope, never a path it would resolve again: it reads exactly the checked file.

use std::os::fd::AsRawFd;
use std::path::PathBuf;
use std::time::Duration;

use base64::Engine;
use md5::{Digest, Md5};
use percent_encoding::{AsciiSet, CONTROLS, utf8_percent_encode};

use super::scope::{Refusal, Scope, home};

const URI: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'#')
    .add(b'%')
    .add(b'<')
    .add(b'>')
    .add(b'?');
const IMAGES: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "webp", "bmp", "tif", "tiff", "svg", "avif", "heic",
];
/// Bigger images are not worth decoding for a 128 px tile.
const MAX_SOURCE: u64 = 64 << 20;

fn cache() -> PathBuf {
    std::env::var_os("XDG_CACHE_HOME").map_or_else(|| home().join(".cache"), PathBuf::from)
}

pub async fn thumb(sc: &Scope, path: &str) -> Result<String, Refusal> {
    let opened = sc.open(path, libc::O_RDONLY)?;
    let md = opened.file.metadata().map_err(|e| e.to_string())?;
    let real = opened.real.to_string_lossy().into_owned();
    let key = hex::encode(Md5::digest(format!(
        "file://{}",
        utf8_percent_encode(&real, URI)
    )));
    let mtime = super::secs(md.modified());
    let ours = cache()
        .join("wado/thumbs")
        .join(format!("{key}-{mtime}.png"));
    for p in [
        cache().join("thumbnails/normal").join(format!("{key}.png")),
        cache().join("thumbnails/large").join(format!("{key}.png")),
        ours.clone(),
    ] {
        // ponytail: a cache entry newer than the file is taken as current; the spec's
        // Thumb::MTime check is the stricter upgrade.
        if let Ok(m) = std::fs::metadata(&p) {
            if super::secs(m.modified()) >= mtime {
                if let Ok(b) = std::fs::read(&p) {
                    return Ok(base64::engine::general_purpose::STANDARD.encode(b));
                }
            }
        }
    }
    let ext = real.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    if !md.is_file() || !IMAGES.contains(&ext.as_str()) || md.len() > MAX_SOURCE {
        return Err("no thumbnail".into());
    }
    let _ = std::fs::create_dir_all(ours.parent().unwrap_or(&ours));
    let src = format!(
        "/proc/{}/fd/{}",
        std::process::id(),
        opened.file.as_raw_fd()
    );
    let run = tokio::process::Command::new("gdk-pixbuf-thumbnailer")
        .args(["-s", "128", &src])
        .arg(&ours)
        .kill_on_drop(true)
        .status();
    match tokio::time::timeout(Duration::from_secs(10), run).await {
        Ok(Ok(st)) if st.success() => {}
        _ => return Err("no thumbnail".into()),
    }
    drop(opened);
    std::fs::read(&ours)
        .map(|b| base64::engine::general_purpose::STANDARD.encode(b))
        .map_err(|_| "no thumbnail".into())
}

mod hex {
    pub fn encode(d: impl AsRef<[u8]>) -> String {
        crate::files::send::hex(d.as_ref())
    }
}
