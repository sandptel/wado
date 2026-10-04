//! The file manager, host side (`WADO_PLAN.md`, Decision Log `2026-10-04`). One job per file:
//!
//! | | |
//! |---|---|
//! | [`scope`] | what a path may reach — `openat2(RESOLVE_BENEATH)` and the denylist |
//! | [`channel`] | the second peer connection and its `files` data channel; who may do what |
//! | [`list`] | a folder's entries |
//! | [`find`] | search by name or kind, bounded |
//! | [`send`], [`zip`] | downloads: a file from an offset, a folder as a STORE zip |
//! | [`recv`] | uploads: `.wado-part`, hash check, rename |
//! | [`ops`], [`trash`] | new folder, rename, move, copy; delete to the freedesktop Trash |
//! | [`media`] | video and audio: probe, stream as fragmented MP4 (remux or transcode), subtitles |
//! | [`quick`], [`thumb`] | pinned folders, recent files; thumbnails |
//! | [`audit`] | the log of every operation, and the toasts it feeds other devices |
//! | [`pace`] | the download rate cap while a session's video is live |
//!
//! The wire is `wado_protocol::files`. The compositor never sees any of this.

pub mod audit;
pub mod channel;
pub mod find;
pub mod list;
pub mod media;
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

/// A helper process for the file manager (ffmpeg, ffprobe, a thumbnailer), fenced off from the
/// session: it runs at the lowest CPU priority, so the compositor and its encoder always win;
/// it dies with the daemon (`PR_SET_PDEATHSIG`), so nothing outlives a crash; and it is killed
/// when its future is dropped (a cancel, a seek, a closed channel).
pub fn tool(prog: &str) -> tokio::process::Command {
    let mut cmd = tokio::process::Command::new(prog);
    cmd.kill_on_drop(true).stdin(std::process::Stdio::null());
    // SAFETY: prctl and setpriority are async-signal-safe and touch only this child.
    unsafe {
        cmd.pre_exec(|| {
            libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL);
            libc::setpriority(libc::PRIO_PROCESS, 0, 19);
            Ok(())
        });
    }
    cmd
}

/// `(free, total)` bytes on the filesystem holding `path`.
pub fn space(path: &std::path::Path) -> Option<(u64, u64)> {
    use std::os::unix::ffi::OsStrExt;
    let c = std::ffi::CString::new(path.as_os_str().as_bytes()).ok()?;
    // SAFETY: `statvfs` is plain data filled by the kernel for a NUL-terminated path.
    unsafe {
        let mut st: libc::statvfs = std::mem::zeroed();
        (libc::statvfs(c.as_ptr(), &mut st) == 0).then(|| {
            let f = st.f_frsize as u64;
            (st.f_bavail as u64 * f, st.f_blocks as u64 * f)
        })
    }
}
