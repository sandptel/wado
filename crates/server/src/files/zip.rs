//! A folder as one uncompressed (STORE) zip, streamed: nothing is staged on disk, and the exact
//! size is known before the first byte, so the device's bar is honest.
//!
//! Not resumable — a drop restarts it (Decision Log `2026-10-04`, item 9). Denied trees inside
//! the folder are left out; symlinks are left out rather than followed.
//!
//! ponytail: no Zip64, so a folder over 4 GiB or 65 535 entries is refused with a pointer to
//! downloading its files instead. Zip64 records are the upgrade path.

use std::io::Read;
use std::sync::atomic::Ordering;

use serde_json::json;
use wado_protocol::files::CHUNK;

use super::channel::Chan;
use super::scope::{Refusal, Scope, say};

const CRC: crc::Crc<u32> = crc::Crc::<u32>::new(&crc::CRC_32_ISO_HDLC);
/// bit 3: CRC and sizes follow the data; bit 11: names are UTF-8.
const FLAGS: u16 = 0x0008 | 0x0800;

struct Item {
    /// The name inside the zip, `/`-separated; a folder ends in `/`.
    name: String,
    /// The host path, for opening.
    path: String,
    size: u64,
    mtime: u64,
}

fn walk(sc: &Scope, path: &str, prefix: &str, out: &mut Vec<Item>) -> Result<(), Refusal> {
    let dir = sc.dir(path)?;
    for e in std::fs::read_dir(dir.here()).map_err(say)?.flatten() {
        let Ok(name) = e.file_name().into_string() else {
            continue;
        };
        if sc.denied(&dir.real.join(&name)) {
            continue;
        }
        let Ok(md) = e.metadata() else { continue };
        let child = format!("{}/{name}", path.trim_end_matches('/'));
        let zname = format!("{prefix}{name}");
        if md.is_dir() {
            out.push(Item {
                name: format!("{zname}/"),
                path: child.clone(),
                size: 0,
                mtime: super::secs(md.modified()),
            });
            walk(sc, &child, &format!("{zname}/"), out)?;
        } else if md.is_file() {
            out.push(Item {
                name: zname,
                path: child,
                size: md.len(),
                mtime: super::secs(md.modified()),
            });
        }
        if out.len() >= 65_535 {
            return Err("too many files to zip — download them separately".into());
        }
    }
    Ok(())
}

fn dos(secs: u64) -> (u16, u16) {
    let t = super::local(secs);
    let year = (t.tm_year + 1900).clamp(1980, 2107) as u16;
    let time = ((t.tm_hour as u16) << 11) | ((t.tm_min as u16) << 5) | (t.tm_sec as u16 / 2);
    let date = ((year - 1980) << 9) | (((t.tm_mon + 1) as u16) << 5) | t.tm_mday as u16;
    (time, date)
}

/// The exact size of the zip of `items`.
fn total(items: &[Item]) -> u64 {
    items
        .iter()
        .map(|i| 30 + 16 + 46 + 2 * i.name.len() as u64 + i.size)
        .sum::<u64>()
        + 22
}

fn le16(v: &mut Vec<u8>, x: u16) {
    v.extend_from_slice(&x.to_le_bytes());
}
fn le32(v: &mut Vec<u8>, x: u32) {
    v.extend_from_slice(&x.to_le_bytes());
}

/// Output, sent in `CHUNK`-sized frames.
struct Out<'a> {
    chan: &'a Chan,
    id: u32,
    buf: Vec<u8>,
    sent: u64,
}

impl Out<'_> {
    async fn put(&mut self, data: &[u8]) -> Result<(), Refusal> {
        self.buf.extend_from_slice(data);
        while self.buf.len() >= CHUNK {
            let rest = self.buf.split_off(CHUNK);
            self.flush_buf().await?;
            self.buf = rest;
        }
        Ok(())
    }
    async fn flush_buf(&mut self) -> Result<(), Refusal> {
        if !self.buf.is_empty() {
            if !self.chan.bytes(self.id, &self.buf).await {
                return Err("stopped".into());
            }
            self.sent += self.buf.len() as u64;
            self.buf.clear();
        }
        Ok(())
    }
}

pub async fn send(chan: &Chan, sc: &Scope, id: u32, path: &str) {
    let mut items = Vec::new();
    let base = path
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or("folder");
    if let Err(e) = walk(sc, path, &format!("{base}/"), &mut items) {
        return chan.fail(id, e).await;
    }
    let size = total(&items);
    if size >= u32::MAX as u64 {
        return chan
            .fail(
                id,
                "over 4 GiB — too big for one zip; download its files instead",
            )
            .await;
    }
    let stop = chan.start(id);
    chan.reply(id, json!({ "size": size })).await;
    let mut out = Out {
        chan,
        id,
        buf: Vec::with_capacity(2 * CHUNK),
        sent: 0,
    };
    let result: Result<(), Refusal> = async {
        let mut central = Vec::new();
        let mut buf = vec![0u8; CHUNK];
        for it in &items {
            if stop.load(Ordering::SeqCst) {
                return Err("stopped".into());
            }
            let offset = (out.sent + out.buf.len() as u64) as u32;
            let (time, date) = dos(it.mtime);
            let mut h = Vec::with_capacity(30 + it.name.len());
            le32(&mut h, 0x0403_4b50);
            le16(&mut h, 20);
            le16(&mut h, FLAGS);
            le16(&mut h, 0);
            le16(&mut h, time);
            le16(&mut h, date);
            h.extend_from_slice(&[0; 12]); // CRC and sizes: in the descriptor
            le16(&mut h, it.name.len() as u16);
            le16(&mut h, 0);
            h.extend_from_slice(it.name.as_bytes());
            out.put(&h).await?;

            let mut digest = CRC.digest();
            if !it.name.ends_with('/') {
                let mut f = sc.open(&it.path, libc::O_RDONLY)?.file;
                let mut left = it.size;
                while left > 0 {
                    let want = (left as usize).min(CHUNK);
                    f.read_exact(&mut buf[..want])
                        .map_err(|_| format!("{} changed while it was being zipped", it.name))?;
                    digest.update(&buf[..want]);
                    out.put(&buf[..want]).await?;
                    left -= want as u64;
                }
            }
            let crc = digest.finalize();
            let mut d = Vec::with_capacity(16);
            le32(&mut d, 0x0807_4b50);
            le32(&mut d, crc);
            le32(&mut d, it.size as u32);
            le32(&mut d, it.size as u32);
            out.put(&d).await?;

            let dir = it.name.ends_with('/');
            let mode: u32 = if dir { 0o40755 } else { 0o100644 };
            le32(&mut central, 0x0201_4b50);
            le16(&mut central, (3 << 8) | 20);
            le16(&mut central, 20);
            le16(&mut central, FLAGS);
            le16(&mut central, 0);
            le16(&mut central, time);
            le16(&mut central, date);
            le32(&mut central, crc);
            le32(&mut central, it.size as u32);
            le32(&mut central, it.size as u32);
            le16(&mut central, it.name.len() as u16);
            central.extend_from_slice(&[0; 8]); // extra, comment, disk, internal attrs
            le32(&mut central, (mode << 16) | if dir { 0x10 } else { 0 });
            le32(&mut central, offset);
            central.extend_from_slice(it.name.as_bytes());
        }
        let cd_at = (out.sent + out.buf.len() as u64) as u32;
        let mut end = Vec::with_capacity(22);
        le32(&mut end, 0x0605_4b50);
        le32(&mut end, 0);
        le16(&mut end, items.len() as u16);
        le16(&mut end, items.len() as u16);
        le32(&mut end, central.len() as u32);
        le32(&mut end, cd_at);
        le16(&mut end, 0);
        out.put(&central).await?;
        out.put(&end).await?;
        out.flush_buf().await
    }
    .await;
    chan.finish(id);
    match &result {
        Ok(()) => chan.reply(id, json!({ "done": true })).await,
        Err(e) if e == "stopped" => return,
        Err(e) => chan.fail(id, e.clone()).await,
    }
    chan.audit("download-zip", path, size, &result);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_announced_size_is_the_real_one() {
        // One file "d/a" of 3 bytes and the folder "d/": headers, data, descriptors, central
        // entries and the end record, counted by hand.
        let items = [
            Item {
                name: "d/".into(),
                path: String::new(),
                size: 0,
                mtime: 0,
            },
            Item {
                name: "d/a".into(),
                path: String::new(),
                size: 3,
                mtime: 0,
            },
        ];
        let by_hand = (30 + 2 + 16 + 46 + 2) + (30 + 3 + 3 + 16 + 46 + 3) + 22;
        assert_eq!(total(&items), by_hand);
    }
}
