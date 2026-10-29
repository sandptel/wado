// SPDX-License-Identifier: AGPL-3.0-only
//! The daemon's log, tailed from its file: `~/.cache/wado/daemon.log` (`scripts/daemon.sh`) or
//! the rig's `daemon-N.log` (`wado tui`, `scripts/rig.sh`), whichever was written last.
//!
//! The encoder tier is read from here too, because nothing outside a session publishes it yet:
//! the compositor logs `pipeline=<tier>` when a session (re)configures, and the relay client
//! `session ready — encoder <mode>`. Said as "last seen in the log", never as live truth.
//!
//! ponytail: re-reads the tail (≤512 KiB) only when the file's size or mtime moved.

use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::PathBuf,
    time::SystemTime,
};

/// How much of the file's end is read — a few thousand lines.
const TAIL: u64 = 512 * 1024;
/// Lines kept.
pub const KEEP: usize = 3000;

#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    pub time: String,
    pub level: String,
    pub target: String,
    pub msg: String,
}

impl Line {
    /// What a person running the computer wants to see: who came in, what they did — the same
    /// list `wado tui`'s activity card filters on.
    pub fn activity(&self) -> bool {
        const ACTIVITY: &[&str] = &[
            "gate:",
            "files:",
            "shells:",
            "config changed",
            "host action",
            "session ready",
            "unpaired",
            "paired",
        ];
        ACTIVITY.iter().any(|a| self.msg.contains(a))
    }
}

/// What the log last said about the encoder.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Encoder {
    /// `hardware` | `software`.
    pub mode: Option<String>,
    /// `vaapi-dmabuf` | `vaapi-cpu` | `x264-cpu`.
    pub pipeline: Option<String>,
    /// When it was said.
    pub at: String,
}

fn cache_log() -> PathBuf {
    std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_default()
                .join(".cache")
        })
        .join("wado/daemon.log")
}

/// The log written most recently, of the places a daemon logs to.
pub fn source() -> Option<PathBuf> {
    let mut candidates = vec![cache_log()];
    if let Ok(rd) = std::fs::read_dir(wado::cli::status::rig_dir()) {
        candidates.extend(rd.flatten().map(|e| e.path()).filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("daemon") && n.ends_with(".log"))
        }));
    }
    candidates
        .into_iter()
        .filter_map(|p| {
            let m = std::fs::metadata(&p).ok()?.modified().ok()?;
            Some((m, p))
        })
        .max_by_key(|(m, _)| *m)
        .map(|(_, p)| p)
}

/// The tail of a log, remembered so an unchanged file is not read again.
#[derive(Default)]
pub struct Tail {
    path: Option<PathBuf>,
    stamp: Option<(u64, SystemTime)>,
    pub lines: std::sync::Arc<Vec<Line>>,
    pub encoder: Encoder,
    /// Bumped whenever `lines` changed.
    pub generation: u64,
}

impl Tail {
    /// Re-read if the newest log moved. True when anything changed.
    pub fn refresh(&mut self) -> bool {
        let Some(path) = source() else {
            let changed = !self.lines.is_empty() || self.path.is_some();
            *self = Tail {
                generation: self.generation + u64::from(changed),
                ..Tail::default()
            };
            return changed;
        };
        let Ok(meta) = std::fs::metadata(&path) else {
            return false;
        };
        let stamp = (
            meta.len(),
            meta.modified().unwrap_or(SystemTime::UNIX_EPOCH),
        );
        if self.path.as_ref() == Some(&path) && self.stamp == Some(stamp) {
            return false;
        }
        let text = read_tail(&path, meta.len()).unwrap_or_default();
        let mut lines: Vec<Line> = text.lines().filter_map(parse).collect();
        if lines.len() > KEEP {
            lines.drain(..lines.len() - KEEP);
        }
        self.encoder = encoder(&lines);
        self.lines = std::sync::Arc::new(lines);
        self.path = Some(path);
        self.stamp = Some(stamp);
        self.generation += 1;
        true
    }

    pub fn path(&self) -> Option<&PathBuf> {
        self.path.as_ref()
    }
}

fn read_tail(path: &PathBuf, len: u64) -> std::io::Result<String> {
    let mut f = File::open(path)?;
    let from = len.saturating_sub(TAIL);
    f.seek(SeekFrom::Start(from))?;
    let mut buf = Vec::with_capacity((len - from) as usize);
    f.read_to_end(&mut buf)?;
    let mut s = String::from_utf8_lossy(&buf).into_owned();
    // Started mid-line: drop the partial first one.
    if from > 0 {
        if let Some(i) = s.find('\n') {
            s.drain(..=i);
        }
    }
    Ok(s)
}

/// `2026-10-29T21:47:16.096352Z  INFO wado::relay_client: relay client: ready` → its parts.
/// ANSI colour is stripped first; a line that does not look like tracing is kept whole.
pub fn parse(raw: &str) -> Option<Line> {
    let l = strip_ansi(raw);
    let l = l.trim_end();
    if l.is_empty() {
        return None;
    }
    let mut it = l.splitn(2, char::is_whitespace);
    let ts = it.next()?;
    let rest = it.next().unwrap_or("").trim_start();
    if ts.len() < 19 || ts.as_bytes().get(10) != Some(&b'T') {
        return Some(Line {
            time: String::new(),
            level: String::new(),
            target: String::new(),
            msg: l.to_string(),
        });
    }
    let mut it = rest.splitn(2, char::is_whitespace);
    let level = it.next().unwrap_or("").to_string();
    let rest = it.next().unwrap_or("").trim_start();
    let (target, msg) = rest.split_once(": ").unwrap_or(("", rest));
    Some(Line {
        time: ts.get(11..19).unwrap_or("").to_string(),
        level,
        target: target.trim_start_matches("wado::").to_string(),
        msg: msg.to_string(),
    })
}

fn encoder(lines: &[Line]) -> Encoder {
    let mut e = Encoder::default();
    for l in lines.iter().rev() {
        if e.pipeline.is_none() {
            if let Some(i) = l.msg.find("pipeline=") {
                let tier: String = l.msg[i + 9..]
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '-')
                    .collect();
                if !tier.is_empty() {
                    e.pipeline = Some(tier);
                    if e.at.is_empty() {
                        e.at = l.time.clone();
                    }
                }
            }
        }
        if e.mode.is_none() {
            if let Some(i) = l.msg.find("— encoder ") {
                let m: String = l.msg[i + "— encoder ".len()..]
                    .chars()
                    .take_while(|c| c.is_ascii_alphabetic())
                    .collect();
                if !m.is_empty() {
                    e.mode = Some(m);
                    if e.at.is_empty() {
                        e.at = l.time.clone();
                    }
                }
            }
        }
        if e.mode.is_some() && e.pipeline.is_some() {
            break;
        }
    }
    e
}

fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            for c in chars.by_ref() {
                if c.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_coloured_tracing_line_parses() {
        let l = parse("\x1b[2m2026-10-29T21:47:16.096352Z\x1b[0m \x1b[32m INFO\x1b[0m \x1b[2mwado::relay_client\x1b[0m\x1b[2m:\x1b[0m relay client: ready").unwrap();
        assert_eq!(l.time, "21:47:16");
        assert_eq!(l.level, "INFO");
        assert_eq!(l.target, "relay_client");
        assert_eq!(l.msg, "relay client: ready");
    }

    #[test]
    fn the_encoder_is_read_from_the_newest_lines() {
        let lines: Vec<Line> = [
            "2026-10-29T18:44:40.662219Z  INFO wado::relay_client: browser: session ready — encoder software",
            "2026-10-29T18:44:41.662219Z  INFO wado::relay_client: browser: session ready — encoder hardware",
            "2026-10-29T18:44:41.859964Z  INFO wado_compositor::headless: session reconfigured to=720x1614@90 pipeline=vaapi-dmabuf",
        ]
        .iter()
        .filter_map(|l| parse(l))
        .collect();
        let e = encoder(&lines);
        assert_eq!(e.mode.as_deref(), Some("hardware"));
        assert_eq!(e.pipeline.as_deref(), Some("vaapi-dmabuf"));
    }
}
