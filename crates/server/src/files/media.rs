//! Video and audio for the viewer: probe a file, then stream it as fragmented MP4 that the
//! browser's Media Source Extensions play while it arrives — any format `ffmpeg` reads.
//!
//! The codecs a browser plays everywhere are H.264 and AAC, so a stream is **remuxed** (copied,
//! no CPU) when the file already is that, and **transcoded** otherwise (libx264 `veryfast`, capped
//! at 1080p; AAC stereo). HEVC is copied when the device says it can decode it.
//!
//! **Seeking** restarts ffmpeg at the new time with `-ss` before `-i` (a fast seek) and
//! `-output_ts_offset`, so the fragments carry the movie's own timestamps and land at the right
//! place in the player's buffer. **Pacing** is credit: the device grants bytes as its buffer
//! drains, ffmpeg's stdout pipe fills, and ffmpeg blocks — a two-hour film is not transcoded
//! ahead of the person watching it.
//!
//! ffmpeg is handed `/proc/<pid>/fd/<n>` of the file this daemon opened through the scope, never
//! a path it would resolve again (as `thumb` does).
//!
//! ponytail: the ffmpeg and ffprobe CLIs, found on PATH — the daemon's linked libav is for the
//! encoder hot path and would be a great deal more code here. Hardware (VA-API) transcode is the
//! upgrade if CPU becomes the limit.

use std::os::fd::AsRawFd;
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use serde_json::{Value, json};
use tokio::io::AsyncReadExt;
use wado_protocol::files::CHUNK;

use super::channel::Chan;
use super::scope::{Opened, Refusal, Scope};

/// Transcodes at once, per daemon: each is a few cores of x264, and a third would only make all
/// of them stutter. Remuxes (copies) take no permit — they cost almost nothing.
static TRANSCODES: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(2);
/// No output from ffmpeg for this long, with the viewer asking for more: it is stuck.
const STALL: Duration = Duration::from_secs(45);

/// Threads for one x264 transcode: half the machine, so the session keeps the other half.
fn threads() -> String {
    let n = std::thread::available_parallelism().map_or(4, |n| n.get());
    (n / 2).clamp(1, 8).to_string()
}

/// Subtitle files looked for beside a video: `film.srt`, `film.en.srt`, …
const SUB_EXT: &[&str] = &["srt", "vtt", "ass", "ssa", "sub"];
/// Subtitle codecs ffmpeg can turn into WebVTT (bitmap ones — PGS, VobSub — cannot).
const TEXT_SUBS: &[&str] = &[
    "subrip", "ass", "ssa", "webvtt", "mov_text", "text", "srt", "microdvd",
];

fn src(o: &Opened) -> String {
    format!("/proc/{}/fd/{}", std::process::id(), o.file.as_raw_fd())
}

async fn ffprobe(o: &Opened) -> Result<Value, Refusal> {
    let out = super::tool("ffprobe")
        .args([
            "-v",
            "error",
            "-print_format",
            "json",
            "-show_format",
            "-show_streams",
        ])
        .arg(src(o))
        .kill_on_drop(true)
        .output();
    let out = tokio::time::timeout(Duration::from_secs(20), out)
        .await
        .map_err(|_| "reading the file's tracks took too long")?
        .map_err(|_| "ffprobe is not installed on the computer — it is needed to play video")?;
    serde_json::from_slice(&out.stdout).map_err(|_| "ffmpeg cannot read this file".into())
}

fn streams<'a>(p: &'a Value, kind: &'a str) -> impl Iterator<Item = &'a Value> + 'a {
    p["streams"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(move |s| s["codec_type"] == kind && s["disposition"]["attached_pic"] != 1)
}

fn tag(s: &Value, k: &str) -> String {
    s["tags"][k].as_str().unwrap_or("").to_string()
}

/// What a file holds, for the player's menus.
pub async fn probe(sc: &Scope, path: &str) -> Result<Value, Refusal> {
    let o = sc.open(path, libc::O_RDONLY)?;
    let p = ffprobe(&o).await?;
    let duration: f64 = p["format"]["duration"]
        .as_str()
        .and_then(|d| d.parse().ok())
        .unwrap_or(0.0);
    let video = streams(&p, "video")
        .next()
        .map(|v| json!({ "codec": v["codec_name"], "width": v["width"], "height": v["height"] }));
    let audio: Vec<Value> = streams(&p, "audio")
        .map(|a| json!({ "codec": a["codec_name"], "lang": tag(a, "language"), "title": tag(a, "title"), "channels": a["channels"] }))
        .collect();
    let subs: Vec<Value> = streams(&p, "subtitle")
        .map(|s| {
            let codec = s["codec_name"].as_str().unwrap_or("");
            json!({ "codec": codec, "lang": tag(s, "language"), "title": tag(s, "title"), "text": TEXT_SUBS.contains(&codec) })
        })
        .collect();
    // Subtitle files beside it: same name up to the first dot, a subtitle extension.
    let (dir, name) = path.rsplit_once('/').unwrap_or(("", path));
    let stem = name.rsplit_once('.').map_or(name, |(s, _)| s);
    let mut sidecars = Vec::new();
    if let Ok(d) = sc.dir(if dir.is_empty() { "/" } else { dir }) {
        for e in std::fs::read_dir(d.here()).into_iter().flatten().flatten() {
            let Ok(n) = e.file_name().into_string() else {
                continue;
            };
            let Some((base, ext)) = n.rsplit_once('.') else {
                continue;
            };
            if SUB_EXT.contains(&ext.to_ascii_lowercase().as_str())
                && (base == stem || base.starts_with(&format!("{stem}.")))
            {
                let lang = base
                    .strip_prefix(stem)
                    .unwrap_or("")
                    .trim_start_matches('.');
                sidecars.push(json!({ "path": format!("{dir}/{n}"), "name": n, "lang": lang }));
            }
        }
    }
    Ok(
        json!({ "duration": duration, "video": video, "audio": audio, "subs": subs, "sidecars": sidecars }),
    )
}

/// The MSE codec string for H.264 as ffprobe describes it, or `None` if a browser will not
/// take it as is (10-bit, 4:2:2, …).
fn avc_codec(v: &Value) -> Option<String> {
    if v["pix_fmt"] != "yuv420p" && v["pix_fmt"] != "yuvj420p" {
        return None;
    }
    let profile = match v["profile"].as_str()? {
        "High" => "6400",
        "Main" => "4d40",
        "Baseline" | "Constrained Baseline" => "42e0",
        _ => return None,
    };
    let level = v["level"].as_i64().filter(|l| (10..=62).contains(l))?;
    Some(format!("avc1.{profile}{level:02x}"))
}

pub async fn stream(
    chan: &Chan,
    sc: &Scope,
    id: u32,
    path: &str,
    start: f64,
    audio: u32,
    hevc: bool,
    credit: Arc<AtomicU64>,
) {
    let o = match sc.open(path, libc::O_RDONLY) {
        Ok(o) => o,
        Err(e) => return chan.fail(id, e).await,
    };
    let p = match ffprobe(&o).await {
        Ok(p) => p,
        Err(e) => return chan.fail(id, e).await,
    };
    let v = streams(&p, "video").next().cloned();
    let a = streams(&p, "audio")
        .nth(audio as usize)
        .cloned()
        .or_else(|| streams(&p, "audio").next().cloned());
    if v.is_none() && a.is_none() {
        return chan.fail(id, "no video or audio in this file").await;
    }
    let start = start.max(0.0);
    let mut args: Vec<String> = ["-hide_banner", "-loglevel", "error", "-nostdin"]
        .map(String::from)
        .to_vec();
    if start > 0.0 {
        args.extend(["-ss".into(), format!("{start:.3}")]);
    }
    args.extend(["-i".into(), src(&o)]);
    let mut codecs = Vec::new();
    if let Some(v) = &v {
        args.extend(["-map".into(), format!("0:{}", v["index"])]);
        let copy = match v["codec_name"].as_str() {
            Some("h264") => avc_codec(v),
            Some("hevc") if hevc => Some("hvc1.1.6.L120.90".into()),
            _ => None,
        };
        match copy {
            Some(c) => {
                args.extend(["-c:v", "copy"].map(String::from));
                if c.starts_with("hvc1") {
                    args.extend(["-tag:v", "hvc1"].map(String::from));
                }
                codecs.push(c);
            }
            None => {
                args.extend(
                    [
                        "-c:v",
                        "libx264",
                        "-preset",
                        "veryfast",
                        "-crf",
                        "22",
                        "-profile:v",
                        "high",
                        "-level",
                        "4.1",
                        "-pix_fmt",
                        "yuv420p",
                        "-vf",
                        "scale='min(1920,iw)':-2",
                        "-g",
                        "48",
                        "-threads",
                    ]
                    .map(String::from),
                );
                args.push(threads());
                codecs.push("avc1.640029".into());
            }
        }
    }
    if let Some(a) = &a {
        args.extend(["-map".into(), format!("0:{}", a["index"])]);
        if a["codec_name"] == "aac" && a["profile"] == "LC" {
            args.extend(["-c:a", "copy"].map(String::from));
        } else {
            args.extend(["-c:a", "aac", "-b:a", "192k", "-ac", "2"].map(String::from));
        }
        codecs.push("mp4a.40.2".into());
    }
    args.extend(["-sn", "-dn", "-map_metadata", "-1"].map(String::from));
    if start > 0.0 {
        args.extend(["-output_ts_offset".into(), format!("{start:.3}")]);
    }
    args.extend(
        [
            "-movflags",
            "frag_keyframe+empty_moov+default_base_moof",
            "-frag_duration",
            "1000000",
            "-f",
            "mp4",
            "pipe:1",
        ]
        .map(String::from),
    );
    // A transcode waits briefly for a slot — a seek's new stream overlaps the old one's ending.
    let _permit = if args.iter().any(|x| x == "libx264") {
        match tokio::time::timeout(Duration::from_secs(5), TRANSCODES.acquire()).await {
            Ok(Ok(p)) => Some(p),
            _ => {
                return chan
                    .fail(
                        id,
                        "the computer is already converting two videos — close one and try again",
                    )
                    .await;
            }
        }
    } else {
        None
    };
    let kind = if v.is_some() { "video" } else { "audio" };
    let mime = format!("{kind}/mp4; codecs=\"{}\"", codecs.join(","));
    let mut child = match super::tool("ffmpeg")
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
    {
        Ok(c) => c,
        Err(_) => {
            return chan
                .fail(
                    id,
                    "ffmpeg is not installed on the computer — it is needed to play video",
                )
                .await;
        }
    };
    let stop = chan.start(id);
    chan.reply(
        id,
        json!({ "mime": mime, "transcode": !args.iter().any(|x| x == "copy") }),
    )
    .await;
    let mut out = child.stdout.take().expect("piped");
    // stderr drained as it comes, keeping the tail: a broken file can make ffmpeg complain on
    // every frame, and an undrained pipe would block it — a stall that looks like the network.
    let mut err_pipe = child.stderr.take().expect("piped");
    let errors = tokio::spawn(async move {
        let mut tail = String::new();
        let mut b = [0u8; 4096];
        while let Ok(n) = err_pipe.read(&mut b).await {
            if n == 0 {
                break;
            }
            tail.push_str(&String::from_utf8_lossy(&b[..n]));
            if tail.len() > 8192 {
                let mut cut = tail.len() - 4096;
                while !tail.is_char_boundary(cut) {
                    cut += 1;
                }
                tail = tail.split_off(cut);
            }
        }
        tail
    });
    let mut buf = vec![0u8; CHUNK];
    let mut sent: u64 = 0;
    let result: Result<(), String> = async {
        loop {
            // Wait for credit: the pipe fills and ffmpeg blocks until the viewer catches up.
            while sent >= credit.load(Ordering::SeqCst) {
                if stop.load(Ordering::SeqCst) || chan.closed() {
                    return Err("stopped".into());
                }
                tokio::time::sleep(Duration::from_millis(40)).await;
            }
            if stop.load(Ordering::SeqCst) {
                return Err("stopped".into());
            }
            let n = tokio::time::timeout(STALL, out.read(&mut buf))
                .await
                .map_err(|_| "ffmpeg stopped producing anything".to_string())?
                .map_err(|e| e.to_string())?;
            if n == 0 {
                return Ok(());
            }
            if !chan.bytes(id, &buf[..n]).await {
                return Err("stopped".into());
            }
            sent += n as u64;
        }
    }
    .await;
    chan.finish(id);
    match result {
        Ok(()) => {
            let status = child.wait().await.ok();
            if status.is_some_and(|s| s.success()) {
                chan.reply(id, json!({ "done": true })).await;
            } else {
                let tail = errors.await.unwrap_or_default();
                let why = tail.lines().last().unwrap_or("ffmpeg failed").to_string();
                chan.fail(id, format!("could not play this file: {why}"))
                    .await;
            }
        }
        Err(e) => {
            let _ = child.kill().await;
            if e != "stopped" {
                chan.fail(id, format!("playback stopped: {e}")).await;
            }
        }
    }
}

/// A subtitle track, or a subtitle file, as WebVTT text.
pub async fn subs(
    sc: &Scope,
    path: &str,
    track: Option<u32>,
    sidecar: Option<&str>,
) -> Result<String, Refusal> {
    let o = sc.open(sidecar.unwrap_or(path), libc::O_RDONLY)?;
    let mut cmd = super::tool("ffmpeg");
    cmd.args(["-hide_banner", "-loglevel", "error", "-nostdin", "-i"])
        .arg(src(&o));
    if sidecar.is_none() {
        cmd.args(["-map", &format!("0:s:{}", track.unwrap_or(0))]);
    }
    cmd.args(["-f", "webvtt", "pipe:1"]).kill_on_drop(true);
    let out = tokio::time::timeout(Duration::from_secs(60), cmd.output())
        .await
        .map_err(|_| "reading the subtitles took too long")?
        .map_err(|_| "ffmpeg is not installed on the computer")?;
    if !out.status.success() {
        return Err(
            "these subtitles cannot be shown (image-based subtitles are not supported yet)".into(),
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn h264_is_copied_only_when_a_browser_takes_it() {
        let v =
            |profile: &str, pix: &str| json!({ "profile": profile, "level": 41, "pix_fmt": pix });
        assert_eq!(
            avc_codec(&v("High", "yuv420p")).as_deref(),
            Some("avc1.640029")
        );
        assert_eq!(
            avc_codec(&v("Main", "yuv420p")).as_deref(),
            Some("avc1.4d4029")
        );
        assert_eq!(avc_codec(&v("High 10", "yuv420p10le")), None);
        assert_eq!(avc_codec(&v("High", "yuv422p")), None);
    }
}
