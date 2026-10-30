//! What a zip holds, for the viewer: its central directory read from the end of the file — the
//! names, sizes and folders, never a byte of the contents. Cheap whatever the archive's size:
//! two bounded reads.
//!
//! Only zip (and the formats that are zips: jar, apk, epub, docx/xlsx/pptx, odt…). A tar has no
//! index — listing one means reading all of it, compressed or not — so the viewer shows an info
//! card for those instead.
//!
//! ponytail: no Zip64. An archive past 4 GiB or 65 535 entries is answered "too big to list
//! here"; Zip64 records are the upgrade path.

use std::io::{Read, Seek, SeekFrom};

use serde_json::{Value, json};

use super::scope::{Refusal, Scope, say};

/// Entries sent at most; the rest are counted.
pub const MAX: usize = 500;
/// The central directory read at most (a zip of tens of thousands of short names fits).
const CD_MAX: u64 = 4 << 20;
/// The end-of-central-directory record: 22 bytes, plus a comment of up to 65 535.
const EOCD_SPAN: u64 = 22 + 65_535;

/// One entry of a zip's central directory.
#[derive(Debug, PartialEq)]
pub struct Entry {
    pub name: String,
    pub size: u64,
    pub dir: bool,
}

/// The listing of the zip at `path`: `{entries:[{name,size,dir}], total, more}`.
pub fn entries(sc: &Scope, path: &str) -> Result<Value, Refusal> {
    let mut f = sc.open(path, libc::O_RDONLY)?.file;
    let (list, total) = read(&mut f)?;
    let more = total > list.len();
    let entries: Vec<_> = list
        .into_iter()
        .map(|e| json!({ "name": e.name, "size": e.size, "dir": e.dir }))
        .collect();
    Ok(json!({ "entries": entries, "total": total, "more": more }))
}

/// The first [`MAX`] entries of a zip, and how many it has.
pub fn read<R: Read + Seek>(f: &mut R) -> Result<(Vec<Entry>, usize), String> {
    let len = f.seek(SeekFrom::End(0)).map_err(say)?;
    let span = len.min(EOCD_SPAN);
    let mut tail = vec![0u8; span as usize];
    f.seek(SeekFrom::Start(len - span)).map_err(say)?;
    f.read_exact(&mut tail).map_err(say)?;
    // The record is the last `PK\x05\x06` whose comment length runs exactly to the end — so the
    // same four bytes inside a comment are not taken for it.
    let at = (0..tail.len().saturating_sub(21))
        .rev()
        .find(|&i| {
            tail[i..i + 4] == [0x50, 0x4b, 0x05, 0x06]
                && i + 22 + u16le(&tail, i + 20) as usize == tail.len()
        })
        .ok_or("this is not a zip archive")?;
    let e = &tail[at..];
    let total = u16le(e, 10) as usize;
    let cd_size = u32le(e, 12) as u64;
    let cd_off = u32le(e, 16) as u64;
    if total == 0xffff || cd_size == 0xffff_ffff || cd_off == 0xffff_ffff {
        return Err("this archive is too big to list here (Zip64)".into());
    }
    if cd_size > CD_MAX || cd_off + cd_size > len {
        return Err("this archive's index is too big or damaged to list here".into());
    }
    let mut cd = vec![0u8; cd_size as usize];
    f.seek(SeekFrom::Start(cd_off)).map_err(say)?;
    f.read_exact(&mut cd).map_err(say)?;
    let mut out = Vec::new();
    let mut p = 0usize;
    while out.len() < MAX && p + 46 <= cd.len() && cd[p..p + 4] == [0x50, 0x4b, 0x01, 0x02] {
        let size = u32le(&cd, p + 24) as u64;
        let n = u16le(&cd, p + 28) as usize;
        let skip = n + u16le(&cd, p + 30) as usize + u16le(&cd, p + 32) as usize;
        let Some(raw) = cd.get(p + 46..p + 46 + n) else {
            break;
        };
        let name = String::from_utf8_lossy(raw).into_owned();
        out.push(Entry {
            dir: name.ends_with('/'),
            name,
            size,
        });
        p += 46 + skip;
    }
    Ok((out, total))
}

fn u16le(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}

fn u32le(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    /// A STORE zip of `files`, with an optional archive comment.
    fn zip(files: &[(&str, &[u8])], comment: &[u8]) -> Vec<u8> {
        let (mut out, mut cd) = (Vec::new(), Vec::new());
        for (name, body) in files {
            let off = out.len() as u32;
            out.extend_from_slice(b"PK\x03\x04");
            out.extend_from_slice(&[20, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
            out.extend_from_slice(&(body.len() as u32).to_le_bytes());
            out.extend_from_slice(&(body.len() as u32).to_le_bytes());
            out.extend_from_slice(&(name.len() as u16).to_le_bytes());
            out.extend_from_slice(&[0, 0]);
            out.extend_from_slice(name.as_bytes());
            out.extend_from_slice(body);
            cd.extend_from_slice(b"PK\x01\x02");
            cd.extend_from_slice(&[20, 0, 20, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
            cd.extend_from_slice(&(body.len() as u32).to_le_bytes());
            cd.extend_from_slice(&(body.len() as u32).to_le_bytes());
            cd.extend_from_slice(&(name.len() as u16).to_le_bytes());
            cd.extend_from_slice(&[0; 12]);
            cd.extend_from_slice(&off.to_le_bytes());
            cd.extend_from_slice(name.as_bytes());
        }
        let cd_off = out.len() as u32;
        out.extend_from_slice(&cd);
        out.extend_from_slice(b"PK\x05\x06\0\0\0\0");
        out.extend_from_slice(&(files.len() as u16).to_le_bytes());
        out.extend_from_slice(&(files.len() as u16).to_le_bytes());
        out.extend_from_slice(&(cd.len() as u32).to_le_bytes());
        out.extend_from_slice(&cd_off.to_le_bytes());
        out.extend_from_slice(&(comment.len() as u16).to_le_bytes());
        out.extend_from_slice(comment);
        out
    }

    #[test]
    fn lists_names_sizes_and_folders() {
        let z = zip(
            &[
                ("photos/", b""),
                ("photos/a.jpg", b"abcde"),
                ("notes.txt", b"hi\n"),
            ],
            b"",
        );
        let (list, total) = read(&mut Cursor::new(z)).unwrap();
        assert_eq!(total, 3);
        assert_eq!(
            list,
            vec![
                Entry {
                    name: "photos/".into(),
                    size: 0,
                    dir: true
                },
                Entry {
                    name: "photos/a.jpg".into(),
                    size: 5,
                    dir: false
                },
                Entry {
                    name: "notes.txt".into(),
                    size: 3,
                    dir: false
                },
            ]
        );
    }

    #[test]
    fn finds_the_record_behind_a_comment() {
        let z = zip(&[("a", b"1")], b"made with PK\x05\x06 inside the comment");
        let (list, total) = read(&mut Cursor::new(z)).unwrap();
        assert_eq!((list.len(), total), (1, 1));
        assert_eq!(list[0].name, "a");
    }

    #[test]
    fn caps_the_listing_and_counts_the_rest() {
        let names: Vec<String> = (0..MAX + 7).map(|i| format!("f{i}")).collect();
        let files: Vec<(&str, &[u8])> = names.iter().map(|n| (n.as_str(), &b"x"[..])).collect();
        let (list, total) = read(&mut Cursor::new(zip(&files, b""))).unwrap();
        assert_eq!((list.len(), total), (MAX, MAX + 7));
    }

    #[test]
    fn refuses_what_is_not_a_zip() {
        assert!(
            read(&mut Cursor::new(
                b"just some text, no archive here".to_vec()
            ))
            .is_err()
        );
        assert!(read(&mut Cursor::new(Vec::new())).is_err());
    }

    #[test]
    fn refuses_an_index_pointing_past_the_end() {
        let mut z = zip(&[("a", b"1")], b"");
        let n = z.len();
        z[n - 6..n - 2].copy_from_slice(&9_999_999u32.to_le_bytes());
        assert!(read(&mut Cursor::new(z)).is_err());
    }
}
