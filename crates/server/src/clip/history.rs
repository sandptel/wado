//! The history, from `cliphist`: newest first, pinned entries on top.
//!
//! An id is cliphist's, sent back by the viewer, so it is checked to be digits before it goes
//! anywhere near a command. Pins are wado's own (cliphist has none), kept in
//! `~/.config/wado/clip_pins` by preview text.
//!
//! ponytail: pinned by preview, not id — copying the same thing again gives it a new id in
//! cliphist, and the preview survives that. Two entries sharing their first 100 characters share
//! a pin; a content hash is the upgrade if that ever matters.

use std::path::PathBuf;

use wado_compositor::clipboard::Clip;
use wado_protocol::{ClipEntry, ClipKind};

use super::tool;

/// How many entries a viewer is sent. cliphist keeps 750 by default.
const SHOWN: usize = 120;
/// A tile's preview, in characters.
const PREVIEW: usize = 280;

pub async fn list() -> Result<Vec<ClipEntry>, String> {
    let out = tool::run("cliphist", &["list"], b"").await?;
    let pins = pins();
    let mut all: Vec<ClipEntry> = String::from_utf8_lossy(&out)
        .lines()
        .filter_map(|l| l.split_once('\t'))
        .take(SHOWN)
        .map(|(id, text)| entry(id, text, &pins))
        .collect();
    all.sort_by_key(|e| !e.pinned); // stable: newest first within each group
    Ok(all)
}

fn entry(id: &str, text: &str, pins: &[String]) -> ClipEntry {
    let t = text.trim();
    let kind = if t.starts_with("[[ binary data") {
        ClipKind::Image
    } else if (t.starts_with("https://") || t.starts_with("http://"))
        && !t.contains(char::is_whitespace)
    {
        ClipKind::Link
    } else {
        ClipKind::Text
    };
    let preview = match kind {
        // `[[ binary data 154 KiB png 1920x1080 ]]` → `png 1920x1080 · 154 KiB`
        ClipKind::Image => image_label(t),
        _ => t.chars().take(PREVIEW).collect(),
    };
    ClipEntry {
        id: id.to_string(),
        kind,
        pinned: pins.iter().any(|p| p == text),
        preview,
    }
}

fn image_label(t: &str) -> String {
    let w: Vec<&str> = t
        .trim_start_matches("[[ binary data")
        .trim_end_matches("]]")
        .split_whitespace()
        .collect();
    match w.as_slice() {
        [n, unit, rest @ ..] => format!("{} · {n} {unit}", rest.join(" ")),
        _ => "image".into(),
    }
}

fn check(id: &str) -> Result<(), String> {
    if !id.is_empty() && id.len() < 20 && id.bytes().all(|b| b.is_ascii_digit()) {
        Ok(())
    } else {
        Err("not a clipboard entry".into())
    }
}

/// Entry `id` in full.
pub async fn get(id: &str) -> Result<Clip, String> {
    check(id)?;
    let bytes = tool::run("cliphist", &["decode"], format!("{id}\t\n").as_bytes()).await?;
    let mime = super::data::sniff(&bytes).unwrap_or("text/plain;charset=utf-8");
    Ok(Clip {
        mime: mime.into(),
        data: bytes.into(),
    })
}

pub async fn delete(id: &str) -> Result<(), String> {
    check(id)?;
    // The pin goes with it.
    let line = line_of(id).await?;
    pin_line(&line, false);
    tool::run("cliphist", &["delete"], format!("{line}\n").as_bytes())
        .await
        .map(drop)
}

pub async fn pin(id: &str, on: bool) -> Result<(), String> {
    check(id)?;
    let line = line_of(id).await?;
    pin_line(&line, on);
    Ok(())
}

/// The `id\tpreview` line cliphist knows entry `id` by.
async fn line_of(id: &str) -> Result<String, String> {
    let out = tool::run("cliphist", &["list"], b"").await?;
    String::from_utf8_lossy(&out)
        .lines()
        .find(|l| l.split_once('\t').is_some_and(|(i, _)| i == id))
        .map(str::to_string)
        .ok_or("that entry is no longer in the history".into())
}

fn pins_file() -> PathBuf {
    crate::remote_id::config_dir().join("clip_pins")
}

fn pins() -> Vec<String> {
    std::fs::read_to_string(pins_file())
        .map(|s| s.lines().map(str::to_string).collect())
        .unwrap_or_default()
}

fn pin_line(line: &str, on: bool) {
    let Some((_, text)) = line.split_once('\t') else {
        return;
    };
    let mut p = pins();
    p.retain(|t| t != text);
    if on {
        p.push(text.to_string());
    }
    let _ = std::fs::write(pins_file(), p.join("\n"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_and_labels() {
        let e = entry("7", "[[ binary data 154 KiB png 1920x1080 ]]", &[]);
        assert_eq!(e.kind, ClipKind::Image);
        assert_eq!(e.preview, "png 1920x1080 · 154 KiB");
        assert_eq!(entry("8", "https://x.y/z", &[]).kind, ClipKind::Link);
        assert_eq!(entry("9", "see https://x.y", &[]).kind, ClipKind::Text);
        assert!(entry("9", "pinned", &["pinned".into()]).pinned);
        assert!(check("12").is_ok() && check("1;rm").is_err() && check("").is_err());
    }
}
