//! Uploads: bytes go to `<name>.wado-part`, the hash is checked, and only then is the file
//! renamed into place. A dropped upload leaves its part behind, and the next attempt carries on
//! from its size.
//!
//! The part is opened inside its folder's descriptor with `O_NOFOLLOW`, so a symlink planted
//! under the part's name cannot redirect the write.

use std::fs::File;
use std::io::{Read, Write};

use ring::digest::{Context, SHA256};
use serde_json::json;
use tracing::warn;
use wado_protocol::files::{CHUNK, Clash};

use super::channel::Chan;
use super::scope::{Opened, Refusal, Scope, check_name, say};

pub struct Upload {
    dir: Opened,
    part: String,
    name: String,
    file: File,
    size: u64,
    written: u64,
    replace: bool,
    path: String,
}

const PART: &str = ".wado-part";

/// Free space where `dir` lives, in bytes.
fn free(dir: &Opened) -> Option<u64> {
    use std::os::fd::AsRawFd;
    // SAFETY: `statvfs` is plain data, filled by the kernel for a valid fd.
    unsafe {
        let mut st: libc::statvfs = std::mem::zeroed();
        (libc::fstatvfs(dir.file.as_raw_fd(), &mut st) == 0)
            .then(|| st.f_bavail as u64 * st.f_frsize as u64)
    }
}

/// The folder an upload lands in, creating the folders of an uploaded tree on the way.
fn landing(sc: &Scope, dir: &str, name: &str) -> Result<(Opened, String, String), Refusal> {
    let parts: Vec<&str> = name.split('/').collect();
    for p in &parts {
        check_name(p)?;
    }
    let (file, folders) = parts.split_last().ok_or("no name")?;
    let mut path = dir.trim_end_matches('/').to_string();
    let mut at = sc.dir(if path.is_empty() { "/" } else { &path })?;
    for f in folders {
        match at.mkdir_at(f) {
            Ok(()) => {}
            Err(e) if e.raw_os_error() == Some(libc::EEXIST) => {}
            Err(e) => return Err(say(e)),
        }
        path = format!("{path}/{f}");
        at = sc.dir(&path)?;
    }
    Ok((at, file.to_string(), path))
}

pub async fn put(chan: &Chan, sc: &Scope, id: u32, dir: &str, name: &str, size: u64, clash: Clash) {
    let r: Result<(Upload, u64), Refusal> = (|| {
        let (at, mut file, folder) = landing(sc, dir, name)?;
        if file.ends_with(PART) {
            return Err(format!("names ending in {PART} are wado's own"));
        }
        if at.has(&file) {
            match clash {
                Clash::Fail => return Err("exists".into()),
                Clash::Rename => file = at.free_name(&file),
                Clash::Replace => {}
            }
        }
        let part = format!("{file}{PART}");
        let f = at
            .open_at(&part, libc::O_WRONLY | libc::O_CREAT, 0o666)
            .map_err(say)?
            .file;
        let mut have = f.metadata().map_err(say)?.len();
        if have > size {
            f.set_len(0).map_err(say)?;
            have = 0;
        }
        let reserve = wado_config::live::current().files.reserve_mib << 20;
        if free(&at).is_some_and(|free| free < (size - have).saturating_add(reserve)) {
            return Err("not enough free space on the computer for this file".into());
        }
        let mut f = f;
        use std::io::Seek;
        f.seek(std::io::SeekFrom::Start(have)).map_err(say)?;
        let path = format!("{folder}/{file}");
        Ok((
            Upload {
                dir: at,
                part,
                name: file,
                file: f,
                size,
                written: have,
                replace: clash == Clash::Replace,
                path,
            },
            have,
        ))
    })();
    match r {
        Ok((up, have)) => {
            let name = up.name.clone();
            chan.ups
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .insert(id, up);
            chan.reply(id, json!({ "name": name, "offset": have }))
                .await;
        }
        Err(e) => chan.fail(id, e).await,
    }
}

/// One binary frame: 4 bytes of transfer id, then data for that upload's part.
pub async fn chunk(chan: &Chan, frame: &[u8]) {
    if frame.len() < 4 {
        return;
    }
    let id = u32::from_be_bytes([frame[0], frame[1], frame[2], frame[3]]);
    let data = &frame[4..];
    let err = {
        let mut ups = chan.ups.lock().unwrap_or_else(|e| e.into_inner());
        let Some(up) = ups.get_mut(&id) else { return };
        let r = if up.written + data.len() as u64 > up.size {
            Err("more bytes than the upload announced".to_string())
        } else {
            up.file.write_all(data).map_err(say)
        };
        match r {
            Ok(()) => {
                up.written += data.len() as u64;
                None
            }
            Err(e) => {
                ups.remove(&id);
                Some(e)
            }
        }
    };
    if let Some(e) = err {
        chan.fail(id, e).await;
    }
}

/// The device says it has sent everything, and what its hash is.
pub async fn end(chan: &Chan, id: u32, xfer: u32, sha256: &str) {
    let Some(up) = chan
        .ups
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(&xfer)
    else {
        return chan.fail(id, "no such upload").await;
    };
    let path = up.path.clone();
    let size = up.size;
    let sha256 = sha256.to_string();
    let sc = super::scope();
    let r = tokio::task::spawn_blocking(move || finish(&sc, up, sha256))
        .await
        .unwrap_or_else(|e| Err(e.to_string()));
    chan.audit("upload", &path, size, &r);
    match r {
        Ok(()) => chan.reply(id, json!({})).await,
        Err(e) => chan.fail(id, e).await,
    }
}

fn finish(sc: &Scope, up: Upload, sha256: String) -> Result<(), Refusal> {
    if up.written != up.size {
        return Err(format!("only {} of {} bytes arrived", up.written, up.size));
    }
    up.file.sync_all().map_err(say)?;
    drop(up.file);
    let mut f = up
        .dir
        .open_at(&up.part, libc::O_RDONLY, 0)
        .map_err(say)?
        .file;
    let mut hash = Context::new(&SHA256);
    let mut buf = vec![0u8; CHUNK];
    loop {
        let n = f.read(&mut buf).map_err(say)?;
        if n == 0 {
            break;
        }
        hash.update(&buf[..n]);
    }
    if super::send::hex(hash.finish().as_ref()) != sha256.to_ascii_lowercase() {
        // A part that does not match is worthless for a resume, too.
        if let Err(e) = std::fs::remove_file(up.dir.at(&up.part)) {
            warn!("files: could not remove a bad part: {e}");
        }
        return Err("the file arrived damaged (hash mismatch) — send it again".into());
    }
    // "Replace" sends the old one to the Trash, as a move or copy does — never lost outright.
    if up.replace && up.dir.has(&up.name) {
        super::trash::trash(sc, &up.path)?;
    }
    up.dir
        .rename_at(&up.part, &up.dir, &up.name, false)
        .map_err(say)
}
