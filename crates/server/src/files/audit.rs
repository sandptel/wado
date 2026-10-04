//! Every file operation, on the record: the daemon log, and an append-only
//! `~/.local/state/wado/files.log` (time, daemon, device, op, path, size, result).
//!
//! The file is also how the pool's *other* devices hear about it (Decision Log `2026-10-04`,
//! item 13): every daemon of a pool runs on this machine, so each follows the log and toasts its
//! own viewer about what other daemons' devices did — no daemon-to-daemon channel needed.
//!
//! ponytail: followed by polling once a second, like the gate's pending dir.

use std::io::{Read, Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::time::Duration;

use tracing::info;

use super::channel::Who;

pub fn log_path() -> PathBuf {
    std::env::var_os("XDG_STATE_HOME")
        .map_or_else(|| super::scope::home().join(".local/state"), PathBuf::from)
        .join("wado/files.log")
}

fn instance() -> String {
    wado_config::live::current().server.instance.clone()
}

fn clean(s: &str) -> String {
    s.replace(['\t', '\n'], " ")
}

pub fn record(who: &Who, op: &str, path: &str, size: u64, result: &Result<(), String>) {
    let res = match result {
        Ok(()) => "ok".to_string(),
        Err(e) => format!("err: {}", clean(e)),
    };
    info!(device = %who.name, op, path, size, result = %res, "files");
    let line = format!(
        "{}\t{}\t{}\t{op}\t{}\t{size}\t{res}\n",
        super::now_s(),
        instance(),
        clean(&who.name),
        clean(path)
    );
    let p = log_path();
    let _ = std::fs::create_dir_all(p.parent().unwrap_or(&p));
    let r = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&p)
        .and_then(|mut f| f.write_all(line.as_bytes()));
    if let Err(e) = r {
        tracing::warn!("files: could not write the audit log: {e}");
    }
}

/// `(device, op, path)` for each successful operation another daemon logs from now on.
pub async fn follow(tx: tokio::sync::mpsc::Sender<(String, String, String)>) {
    let p = log_path();
    let mut pos = std::fs::metadata(&p).map_or(0, |m| m.len());
    let me = instance();
    loop {
        tokio::time::sleep(Duration::from_secs(1)).await;
        let Ok(mut f) = std::fs::File::open(&p) else {
            continue;
        };
        let len = f.metadata().map_or(0, |m| m.len());
        if len < pos {
            pos = 0; // rotated
        }
        if len == pos || f.seek(SeekFrom::Start(pos)).is_err() {
            continue;
        }
        let mut text = String::new();
        if f.read_to_string(&mut text).is_err() {
            continue;
        }
        // Only whole lines; a half-written one is read next time.
        let upto = text.rfind('\n').map_or(0, |i| i + 1);
        pos += upto as u64;
        for l in text[..upto].lines() {
            let f: Vec<&str> = l.split('\t').collect();
            if f.len() >= 7 && f[1] != me && f[6] == "ok" {
                if tx
                    .send((f[2].into(), f[3].into(), f[4].into()))
                    .await
                    .is_err()
                {
                    return;
                }
            }
        }
    }
}
