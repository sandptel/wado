//! The Mac's "now playing", whatever app it is, through mediaremote-adapter
//! (github.com/ungive/mediaremote-adapter, BSD-3-Clause; built by `scripts/mac-media-adapter.sh`,
//! bundled in `Wado.app/Contents/Resources/media-adapter`).
//!
//! Since macOS 15.4 only Apple-entitled processes may read MediaRemote; Apple's `/usr/bin/perl`
//! is one, so the adapter's script runs under it, loads the adapter's framework, and streams the
//! now-playing state as JSON lines — each a full state or a diff over the last one. Commands
//! (play/pause, skip, seek, shuffle, repeat) go the same way, to whichever app is now playing.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::{Map, Value};
use wado_protocol::host::{MediaOp, Player};

use crate::art;

const PERL: &str = "/usr/bin/perl";
const SCRIPT: &str = "mediaremote-adapter.pl";
const FRAMEWORK: &str = "MediaRemoteAdapter.framework";

/// The adapter's two files, found next to the executable.
#[derive(Clone)]
pub struct Adapter {
    script: PathBuf,
    framework: PathBuf,
}

impl Adapter {
    /// In the app bundle (`Contents/Resources/media-adapter`), else beside a dev build
    /// (`target/media-adapter`, for `target/release/wado`). Why not, when neither has it.
    pub fn find() -> Result<Adapter, String> {
        let exe = std::env::current_exe().map_err(|e| format!("no executable path: {e}"))?;
        let dir = exe.parent().ok_or("the executable has no folder")?;
        [
            dir.join("../Resources/media-adapter"),
            dir.join("../media-adapter"),
        ]
        .into_iter()
        .find_map(|d| Adapter::at(&d))
        .ok_or_else(|| {
            format!(
                "mediaremote-adapter is not in {}/../Resources/media-adapter \
                 (build it with scripts/mac-media-adapter.sh)",
                dir.display()
            )
        })
    }

    fn at(dir: &Path) -> Option<Adapter> {
        let dir = dir.canonicalize().ok()?;
        let a = Adapter {
            script: dir.join(SCRIPT),
            framework: dir.join(FRAMEWORK),
        };
        (a.script.is_file() && a.framework.is_dir()).then_some(a)
    }

    fn args<'a>(&'a self, rest: &[&'a str]) -> Vec<&'a str> {
        let mut v = vec![
            self.script.to_str().unwrap_or_default(),
            self.framework.to_str().unwrap_or_default(),
        ];
        v.extend_from_slice(rest);
        v
    }

    /// Do `op` to the app now playing.
    pub async fn act(&self, op: &MediaOp) -> Result<(), String> {
        let arg;
        let rest: [&str; 2] = match op {
            MediaOp::PlayPause => ["send", "2"],
            MediaOp::Next => ["send", "4"],
            MediaOp::Previous => ["send", "5"],
            MediaOp::SeekTo { ms } => {
                arg = (ms * 1000).to_string();
                ["seek", &arg]
            }
            MediaOp::Shuffle { on } => ["shuffle", if *on { "3" } else { "1" }],
            MediaOp::Loop { mode } => [
                "repeat",
                match mode.as_str() {
                    "Track" => "2",
                    "Playlist" => "3",
                    _ => "1",
                },
            ],
        };
        crate::cmd::run(PERL, &self.args(&rest), Duration::from_secs(4))
            .await
            .map(drop)
    }

    /// Stream the now-playing state on a thread of its own, calling `on` after every update.
    /// The adapter is restarted with backoff if it exits, and killed when the [`Stream`] drops.
    pub fn stream(self, on: impl Fn(&NowPlaying) + Send + 'static) -> Stream {
        let s = Stream {
            now: Arc::default(),
            child: Arc::default(),
            stop: Arc::default(),
        };
        let (now, child, stop) = (s.now.clone(), s.child.clone(), s.stop.clone());
        std::thread::Builder::new()
            .name("now-playing".into())
            .spawn(move || {
                let mut wait = Duration::from_secs(1);
                while !stop.load(Ordering::Relaxed) {
                    let started = Instant::now();
                    if let Err(e) = self.run(&now, &child, &on) {
                        tracing::warn!("now playing: mediaremote-adapter: {e}");
                    }
                    if stop.load(Ordering::Relaxed) {
                        break;
                    }
                    // A run that held up a while starts the backoff over.
                    if started.elapsed() > Duration::from_secs(60) {
                        wait = Duration::from_secs(1);
                    }
                    tracing::warn!("now playing: adapter stopped; again in {}s", wait.as_secs());
                    std::thread::sleep(wait);
                    wait = (wait * 2).min(Duration::from_secs(60));
                }
            })
            .expect("a thread for now playing");
        s
    }

    /// One run of `stream`, until the adapter exits.
    fn run(
        &self,
        now: &Mutex<NowPlaying>,
        child: &Mutex<Option<Child>>,
        on: &impl Fn(&NowPlaying),
    ) -> Result<(), String> {
        let mut c = Command::new(PERL)
            .args(self.args(&["stream", "--micros"]))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            // Its stderr lines are its error messages: into the daemon's log.
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|e| format!("{PERL}: {e}"))?;
        let out = c.stdout.take().ok_or("no stdout")?;
        *lock(child) = Some(c);
        for line in BufReader::new(out).lines() {
            let line = line.map_err(|e| e.to_string())?;
            let mut n = lock(now);
            if n.apply(&line) {
                on(&n);
            }
        }
        let status = lock(child).take().map(|mut c| c.wait());
        *lock(now) = NowPlaying::default();
        on(&lock(now));
        Err(format!("exited ({status:?})"))
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// The running stream; dropping it stops the adapter.
pub struct Stream {
    now: Arc<Mutex<NowPlaying>>,
    child: Arc<Mutex<Option<Child>>>,
    stop: Arc<AtomicBool>,
}

impl Stream {
    /// The app now playing, its position as of this moment.
    pub fn player(&self) -> Option<Player> {
        lock(&self.now).player(epoch_micros())
    }
}

impl Drop for Stream {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(c) = lock(&self.child).as_mut() {
            let _ = c.kill();
        }
    }
}

pub fn epoch_micros() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0.0, |d| d.as_micros() as f64)
}

/// The now-playing state as the adapter last told it: its keys (with `--micros`), and the
/// artwork already made a small `data:` URI.
#[derive(Default)]
pub struct NowPlaying {
    info: Map<String, Value>,
    art: Option<String>,
}

impl NowPlaying {
    /// Take one line of `stream` output; false when it is not one.
    pub fn apply(&mut self, line: &str) -> bool {
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            return false;
        };
        let Some(payload) = v.get("payload").and_then(Value::as_object) else {
            return false;
        };
        if v.get("diff").and_then(Value::as_bool) != Some(true) {
            *self = NowPlaying::default();
        }
        for (k, val) in payload {
            if k == "artworkData" {
                self.art = val.as_str().and_then(art::data_uri);
            } else if val.is_null() {
                self.info.remove(k);
            } else {
                self.info.insert(k.clone(), val.clone());
            }
        }
        true
    }

    /// Its card, with the position extrapolated to `now` (µs since the epoch) while playing.
    /// `app` is the bundle id; the caller names it.
    pub fn player(&self, now: f64) -> Option<Player> {
        let s = |k| self.info.get(k).and_then(Value::as_str);
        let f = |k| self.info.get(k).and_then(Value::as_f64);
        // A web page plays in WebKit's GPU process; the browser is its parent.
        let bus = s("parentApplicationBundleIdentifier").or(s("bundleIdentifier"))?;
        let playing = self.info.get("playing").and_then(Value::as_bool) == Some(true);
        let length = f("durationMicros").unwrap_or(0.0).max(0.0);
        let mut pos = f("elapsedTimeMicros").unwrap_or(0.0);
        if playing && let Some(t) = f("timestampEpochMicros") {
            pos += (now - t).max(0.0) * f("playbackRate").unwrap_or(1.0);
        }
        if length > 0.0 {
            pos = pos.min(length);
        }
        let mode = |k| f(k).map(|m| m as u8);
        Some(Player {
            bus: bus.into(),
            app: bus.into(),
            title: s("title")?.into(),
            artist: s("artist").unwrap_or_default().into(),
            art: self.art.clone(),
            playing,
            position_ms: (pos.max(0.0) / 1000.0) as u64,
            length_ms: (length / 1000.0) as u64,
            // MediaRemote's modes: 1 off; shuffle 2 albums, 3 tracks; repeat 2 track, 3 all.
            shuffle: mode("shuffleMode").map(|m| m >= 2),
            loop_status: mode("repeatMode").map(|m| {
                match m {
                    2 => "Track",
                    3 => "Playlist",
                    _ => "None",
                }
                .into()
            }),
            can_next: true,
            can_prev: true,
            can_seek: length > 0.0,
            ..Player::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Captured from the adapter (`stream --micros`) on macOS 26.6 while YouTube played in
    // Safari, paused and resumed; the artwork is a 1×1 PNG in place of the real 250 KiB TIFF.
    const FULL: &str = r#"{"type":"data","diff":false,"payload":{"playbackRate":1,"timestampEpochMicros":1794238219976042,"album":"","elapsedTimeMicros":46790000000,"playing":true,"bundleIdentifier":"com.apple.WebKit.GPU","processIdentifier":13972,"parentApplicationBundleIdentifier":"com.apple.Safari","artworkData":"iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==","title":"lofi hip hop radio 📚 beats to relax\/study to","uniqueIdentifier":1496310,"artworkMimeType":"image\/jpeg","durationMicros":50390005140,"artist":"Lofi Girl","contentItemIdentifier":"1496310"}}"#;
    const PAUSED: &str = r#"{"type":"data","diff":true,"payload":{"playing":false}}"#;
    const PAUSED_AT: &str = r#"{"type":"data","diff":true,"payload":{"elapsedTimeMicros":46902766737,"playbackRate":0,"timestampEpochMicros":1794238332996528,"artworkMimeType":"image\/tiff"}}"#;
    const RESUMED: &str = r#"{"type":"data","diff":true,"payload":{"playing":true}}"#;
    const NOTHING: &str = r#"{"type":"data","diff":false,"payload":{}}"#;

    #[test]
    fn a_browser_tab_is_its_browser() {
        let mut n = NowPlaying::default();
        assert!(n.apply(FULL));
        let p = n.player(1794238219976042.0 + 2_000_000.0).unwrap();
        assert_eq!(
            (p.bus.as_str(), p.title.as_str(), p.artist.as_str()),
            (
                "com.apple.Safari",
                "lofi hip hop radio 📚 beats to relax/study to",
                "Lofi Girl"
            )
        );
        assert!(p.playing && p.can_seek);
        assert_eq!((p.position_ms, p.length_ms), (46_792_000, 50_390_005));
        assert!(p.art.unwrap().starts_with("data:image/jpeg;base64,"));
        assert_eq!((p.shuffle, p.loop_status), (None, None));
    }

    #[test]
    fn diffs_pause_and_resume() {
        let mut n = NowPlaying::default();
        for l in [FULL, PAUSED, PAUSED_AT] {
            assert!(n.apply(l));
        }
        // Paused: the position holds, however late it is read.
        let p = n.player(1794238332996528.0 + 10_000_000.0).unwrap();
        assert_eq!((p.playing, p.position_ms), (false, 46_902_766));
        assert!(p.art.is_some(), "a diff without artwork keeps it");
        n.apply(RESUMED);
        assert!(n.player(0.0).unwrap().playing);
    }

    #[test]
    fn nothing_playing_is_no_card() {
        let mut n = NowPlaying::default();
        n.apply(FULL);
        assert!(n.apply(NOTHING));
        assert!(n.player(0.0).is_none());
        assert!(!n.apply("not json"));
    }

    /// The real adapter (`scripts/mac-media-adapter.sh` first), while something plays:
    /// `cargo test -p wado-host-macos live_now_playing -- --ignored --nocapture`.
    #[test]
    #[ignore = "runs the adapter; something must be playing"]
    fn live_now_playing() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../target/media-adapter");
        let a = Adapter::at(&dir).expect("scripts/mac-media-adapter.sh");
        let (tx, rx) = std::sync::mpsc::channel();
        let _s = a.stream(move |n| drop(tx.send(n.player(epoch_micros()))));
        let p = std::iter::from_fn(|| rx.recv_timeout(Duration::from_secs(10)).ok())
            .flatten()
            .find(|p| p.art.is_some())
            .expect("a player with art");
        let art = p.art.as_deref().unwrap_or_default();
        eprintln!(
            "{} — {} [{}] playing={} {}/{} ms, art {} bytes: {}…",
            p.title,
            p.artist,
            p.bus,
            p.playing,
            p.position_ms,
            p.length_ms,
            art.len(),
            &art[..40]
        );
    }

    #[test]
    fn modes_and_a_vanished_key() {
        let mut n = NowPlaying::default();
        n.apply(FULL);
        n.apply(r#"{"type":"data","diff":true,"payload":{"shuffleMode":3,"repeatMode":2,"artist":null}}"#);
        let p = n.player(0.0).unwrap();
        assert_eq!(
            (p.shuffle, p.loop_status.as_deref()),
            (Some(true), Some("Track"))
        );
        assert_eq!(p.artist, "");
    }
}
