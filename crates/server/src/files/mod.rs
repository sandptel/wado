//! The file manager, host side (`WADO_PLAN.md`, Decision Log `2026-10-04`). One job per file:
//!
//! | | |
//! |---|---|
//! | [`scope`] | what a path may reach — `openat2(RESOLVE_BENEATH)` and the denylist |
//! | [`channel`] | the second peer connection and its `files` data channel; who may do what |
//! | [`list`] | a folder's entries |
//! | [`send`], [`zip`] | downloads: a file from an offset, a folder as a STORE zip |
//! | [`recv`] | uploads: `.wado-part`, hash check, rename |
//! | [`ops`], [`trash`] | new folder, rename, move, copy; delete to the freedesktop Trash |
//! | [`quick`], [`thumb`] | pinned folders, recent files; thumbnails |
//! | [`audit`] | the log of every operation, and the toasts it feeds other devices |
//! | [`pace`] | the download rate cap while a session's video is live |
//!
//! The wire is `wado_protocol::files`. The compositor never sees any of this.

pub mod audit;
pub mod channel;
pub mod list;
pub mod ops;
pub mod pace;
pub mod quick;
pub mod recv;
pub mod scope;
pub mod send;
pub mod thumb;
pub mod trash;
pub mod zip;

use std::time::UNIX_EPOCH;

/// Seconds since the epoch of a metadata time, 0 when unknown.
pub fn secs(t: std::io::Result<std::time::SystemTime>) -> u64 {
    t.ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs())
}

/// The scope in force now: the config is re-read per request, so an edit applies at once.
pub fn scope() -> scope::Scope {
    scope::Scope::new(&wado_config::live::current().files)
}

/// Local calendar time of `secs`.
pub fn local(secs: u64) -> libc::tm {
    let t = secs as libc::time_t;
    // SAFETY: `tm` is plain data and `localtime_r` fills it from a valid `time_t`.
    unsafe {
        let mut tm: libc::tm = std::mem::zeroed();
        libc::localtime_r(&t, &mut tm);
        tm
    }
}

pub fn now_s() -> u64 {
    secs(Ok(std::time::SystemTime::now()))
}
