//! Downloads: one file, from an offset, then the SHA-256 of all of it.
//!
//! A resume asks again with `offset` = what the device already has. The hash covers the whole
//! file — the prefix is re-read for it — so a resume that spliced two different versions of a
//! file fails the device's check instead of saving a corrupt one.
//!
//! ponytail: reads are plain blocking reads on the async task — 60 KiB from a local disk at a
//! time. `spawn_blocking` per chunk is the upgrade if a slow disk ever stalls the runtime.

use std::io::{Read, Seek, SeekFrom};
use std::sync::atomic::Ordering;

use ring::digest::{Context, SHA256};
use serde_json::json;
use wado_protocol::files::CHUNK;

use super::channel::Chan;
use super::scope::{Scope, say};

pub fn hex(d: &[u8]) -> String {
    d.iter().map(|b| format!("{b:02x}")).collect()
}

pub async fn get(chan: &Chan, sc: &Scope, id: u32, path: &str, offset: u64) {
    let opened = match sc.open(path, libc::O_RDONLY) {
        Ok(o) => o,
        Err(e) => return chan.fail(id, e).await,
    };
    let mut f = opened.file;
    let md = match f.metadata() {
        Ok(m) if m.is_file() => m,
        Ok(_) => return chan.fail(id, "only regular files can be downloaded").await,
        Err(e) => return chan.fail(id, say(e)).await,
    };
    let size = md.len();
    if offset > size {
        return chan
            .fail(id, "the file is shorter than what was already downloaded")
            .await;
    }
    let stop = chan.start(id);
    chan.reply(
        id,
        json!({ "size": size, "mtime": super::secs(md.modified()) }),
    )
    .await;

    let mut hash = Context::new(&SHA256);
    let mut buf = vec![0u8; CHUNK];
    let mut pos = 0u64;
    let result: Result<(), String> = async {
        // The prefix the device already has: hashed, not sent.
        while pos < offset {
            let want = ((offset - pos) as usize).min(CHUNK);
            f.read_exact(&mut buf[..want]).map_err(say)?;
            hash.update(&buf[..want]);
            pos += want as u64;
        }
        f.seek(SeekFrom::Start(offset)).map_err(say)?;
        while pos < size {
            if stop.load(Ordering::SeqCst) {
                return Err("stopped".into());
            }
            let want = ((size - pos) as usize).min(CHUNK);
            f.read_exact(&mut buf[..want])
                .map_err(|_| "the file changed while it was being sent".to_string())?;
            hash.update(&buf[..want]);
            if !chan.bytes(id, &buf[..want]).await {
                return Err("stopped".into());
            }
            pos += want as u64;
        }
        Ok(())
    }
    .await;
    chan.finish(id);
    match &result {
        Ok(()) => {
            let sha = hex(hash.finish().as_ref());
            chan.reply(id, json!({ "done": true, "sha256": sha })).await;
        }
        Err(e) if e == "stopped" => {}
        Err(e) => chan.fail(id, e.clone()).await,
    }
    // A pause is not worth a line, nor is a resume: the first attempt and failures are logged.
    let stopped = matches!(&result, Err(e) if e == "stopped");
    if !stopped && (offset == 0 || result.is_err()) {
        chan.audit("download", path, size, &result);
    }
}
