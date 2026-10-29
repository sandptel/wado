//! Thumbnails for the file manager: the file is reached through the scope, then made a
//! thumbnail by the platform (`wado_platform::files::Thumbnailer`), which only ever reads the
//! file the scope opened.

use base64::Engine;
use wado_platform::files::ThumbSource;

use super::scope::{Refusal, Scope};

/// A small PNG of `path`, base64, or why there is none.
pub async fn thumb(sc: &Scope, path: &str) -> Result<String, Refusal> {
    let maker = crate::platform::get().thumbnailer().map_err(|a| a.why)?;
    let opened = sc.open(path, libc::O_RDONLY)?;
    let md = opened.file.metadata().map_err(|e| e.to_string())?;
    let png = maker
        .thumb(ThumbSource {
            file: opened.file,
            real: opened.real,
            mtime: super::secs(md.modified()),
        })
        .await?;
    Ok(base64::engine::general_purpose::STANDARD.encode(png))
}
