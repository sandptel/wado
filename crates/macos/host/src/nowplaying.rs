//! The Mac's "now playing", whatever app it is, through our own helper
//! (`crates/macos/host/nowplaying-helper/nowplaying.m`, built by `scripts/mac-app.sh` into
//! `Wado.app/Contents/Resources/libwado-nowplaying.dylib`).
//!
//! Since macOS 15.4 only Apple's own processes may read MediaRemote; Apple's `/usr/bin/perl` is
//! one, so the helper is loaded into it and streams the now-playing state as JSON lines, each the
//! whole state. Commands (play/pause, skip, seek, shuffle, repeat) go the same way, to whichever
//! app is now playing.

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
const LIB: &str = "libwado-nowplaying.dylib";
/// Perl, given the library's path and then the helper's arguments, loads it and calls into it.
const LOADER: &str = r#"DynaLoader::dl_install_xsub("np", DynaLoader::dl_find_symbol(DynaLoader::dl_load_file($ARGV[0]) || die(DynaLoader::dl_error()), "wado_nowplaying") || die(DynaLoader::dl_error())); np()"#;

/// The helper library, found next to the executable.
#[derive(Clone)]
pub struct Adapter {
    lib: PathBuf,
}

impl Adapter {
    /// In the app bundle (`Contents/Resources`), else beside a dev build (`target/release`).
    /// Why not, when neither has it.
    pub fn find() -> Result<Adapter, String> {
        let exe = std::env::current_exe().map_err(|e| format!("no executable path: {e}"))?;
        let dir = exe.parent().ok_or("the executable has no folder")?;
        [dir.join("../Resources").join(LIB), dir.join(LIB)]
            .into_iter()
            .find_map(|p| Adapter::at(&p))
            .ok_or_else(|| {
                format!(
                    "{LIB} is not in {}/../Resources (build it with scripts/mac-app.sh)",
                    dir.display()
                )
            })
    }

    fn at(lib: &Path) -> Option<Adapter> {
        // Canonical: the helper finds its arguments after its own path as the loader gave it.
        let lib = lib.canonicalize().ok()?;
        lib.is_file().then_some(Adapter { lib })
    }

    fn args<'a>(&'a self, rest: &[&'a str]) -> Vec<&'a str> {
        let lib = self.lib.to_str().unwrap_or_default();
        [&["-MDynaLoader", "-e", LOADER, lib], rest].concat()
    }

    /// Do `op` to the app now playing.
    pub async fn act(&self, op: &MediaOp) -> Result<(), String> {
        let secs;
        let rest: &[&str] = match op {
            MediaOp::PlayPause => &["toggle"],
            MediaOp::Next => &["next"],
            MediaOp::Previous => &["previous"],
            MediaOp::SeekTo { ms } => {
                secs = (*ms as f64 / 1000.0).to_string();
                &["seek", &secs]
            }
            // MediaRemote's modes: 1 off; shuffle 3 tracks; repeat 2 track, 3 all.
            MediaOp::Shuffle { on } => &["shuffle", if *on { "3" } else { "1" }],
            MediaOp::Loop { mode } => &[
                "repeat",
                match mode.as_str() {
                    "Track" => "2",
                    "Playlist" => "3",
                    _ => "1",
                },
            ],
        };
        crate::cmd::run(PERL, &self.args(rest), Duration::from_secs(4))
            .await
            .map(drop)
    }

    /// Stream the now-playing state on a thread of its own, calling `on` after every update.
    /// The helper is restarted with backoff if it exits, and killed when the [`Stream`] drops.
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
                        tracing::warn!("now playing: helper: {e}");
                    }
                    if stop.load(Ordering::Relaxed) {
                        break;
                    }
                    // A run that held up a while starts the backoff over.
                    if started.elapsed() > Duration::from_secs(60) {
                        wait = Duration::from_secs(1);
                    }
                    tracing::warn!("now playing: helper stopped; again in {}s", wait.as_secs());
                    std::thread::sleep(wait);
                    wait = (wait * 2).min(Duration::from_secs(60));
                }
            })
            .expect("a thread for now playing");
        s
    }

    /// One run of `stream`, until the helper exits.
    fn run(
        &self,
        now: &Mutex<NowPlaying>,
        child: &Mutex<Option<Child>>,
        on: &impl Fn(&NowPlaying),
    ) -> Result<(), String> {
        let mut c = Command::new(PERL)
            .args(self.args(&["stream"]))
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

/// The running stream; dropping it stops the helper.
pub struct Stream {
    now: Arc<Mutex<NowPlaying>>,
    child: Arc<Mutex<Option<Child>>>,
    stop: Arc<AtomicBool>,
}

impl Stream {
    /// The app now playing, its position as of this moment.
    pub fn player(&self) -> Option<Player> {
        lock(&self.now).player(epoch_secs())
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

pub fn epoch_secs() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0.0, |d| d.as_secs_f64())
}

/// The now-playing state as the helper last told it (its keys are in `nowplaying.m`), and the
/// artwork already made a small `data:` URI.
#[derive(Default)]
pub struct NowPlaying {
    info: Map<String, Value>,
    art: Option<String>,
}

impl NowPlaying {
    /// Take one line of `stream` output; false when it is not one.
    pub fn apply(&mut self, line: &str) -> bool {
        let Ok(Value::Object(mut info)) = serde_json::from_str(line) else {
            return false;
        };
        let same = ["title", "bundle"].map(|k| info.get(k) == self.info.get(k)) == [true; 2];
        if same
            && !info.contains_key("art")
            && let Some(a) = self.info.get("art")
        {
            // MediaRemote drops a track's artwork for a moment and loads it again: keep it.
            info.insert("art".into(), a.clone());
        }
        if info.get("art") != self.info.get("art") {
            self.art = info
                .get("art")
                .and_then(Value::as_str)
                .and_then(art::data_uri);
        }
        self.info = info;
        true
    }

    /// Its card, with the position extrapolated to `now` (seconds since the epoch) while
    /// playing. `app` is the bundle id; the caller names it.
    pub fn player(&self, now: f64) -> Option<Player> {
        let s = |k| self.info.get(k).and_then(Value::as_str);
        let f = |k| self.info.get(k).and_then(Value::as_f64);
        // A web page plays in WebKit's GPU process; the browser is its parent.
        let bus = s("parent").or(s("bundle"))?;
        let playing = self.info.get("playing").and_then(Value::as_bool) == Some(true);
        let length = f("duration").unwrap_or(0.0).max(0.0);
        let mut pos = f("elapsed").unwrap_or(0.0);
        if playing && let Some(t) = f("at") {
            pos += (now - t).max(0.0) * f("rate").unwrap_or(1.0);
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
            position_ms: (pos.max(0.0) * 1000.0) as u64,
            length_ms: (length * 1000.0) as u64,
            // MediaRemote's modes: 1 off; shuffle 2 albums, 3 tracks; repeat 2 track, 3 all.
            shuffle: mode("shuffle").map(|m| m >= 2),
            loop_status: mode("repeat").map(|m| {
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

    // Captured from the helper (`stream`) on macOS 26.6 while YouTube played in Safari, was
    // paused and resumed; the artwork is a 1×1 PNG in place of the real 250 KiB TIFF.
    const FULL: &str = r#"{"album":"","art":"iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==","parent":"com.apple.Safari","bundle":"com.apple.WebKit.GPU","title":"Rick Astley - Never Gonna Give You Up (Official Video) (4K Remaster)","artMime":"image\/jpeg","at":1794246553.6903119,"elapsed":0,"duration":213.06100000000001,"rate":1,"artist":"Rick Astley","playing":true}"#;
    // As captured, but for the artwork gone: MediaRemote drops it for a moment.
    const PAUSED: &str = r#"{"album":"","parent":"com.apple.Safari","bundle":"com.apple.WebKit.GPU","title":"Rick Astley - Never Gonna Give You Up (Official Video) (4K Remaster)","at":1794246555.8864551,"elapsed":2.1055546673557548,"duration":213.06100000000001,"rate":0,"artist":"Rick Astley","playing":false}"#;
    const RESUMED: &str = r#"{"album":"","parent":"com.apple.Safari","bundle":"com.apple.WebKit.GPU","title":"Rick Astley - Never Gonna Give You Up (Official Video) (4K Remaster)","at":1794246557.8122821,"elapsed":2.106889625,"duration":213.06100000000001,"rate":1,"artist":"Rick Astley","playing":true}"#;
    // A new track, its artwork not yet loaded.
    const NEXT: &str = r#"{"bundle":"com.apple.WebKit.GPU","artist":"Pepsodent India","elapsed":0,"parent":"com.apple.Safari","rate":1,"title":"Pepsodent Gum Care Toothpaste_ Gums ki Jalan_Zinc Tech_100% Veg toothpaste_H6s","playing":true,"album":"","at":1794246541.5745869}"#;
    const NOTHING: &str = "{}";

    #[test]
    fn a_browser_tab_is_its_browser() {
        let mut n = NowPlaying::default();
        assert!(n.apply(FULL));
        let p = n.player(1794246553.6903119 + 2.0).unwrap();
        assert_eq!(
            (p.bus.as_str(), p.title.as_str(), p.artist.as_str()),
            (
                "com.apple.Safari",
                "Rick Astley - Never Gonna Give You Up (Official Video) (4K Remaster)",
                "Rick Astley"
            )
        );
        assert!(p.playing && p.can_seek);
        assert_eq!((p.position_ms, p.length_ms), (2_000, 213_061));
        assert!(p.art.unwrap().starts_with("data:image/jpeg;base64,"));
        assert_eq!((p.shuffle, p.loop_status), (None, None));
    }

    #[test]
    fn pause_and_resume() {
        let mut n = NowPlaying::default();
        for l in [FULL, PAUSED] {
            assert!(n.apply(l));
        }
        // Paused: the position holds, however late it is read.
        let p = n.player(1794246555.8864551 + 10.0).unwrap();
        assert_eq!((p.playing, p.position_ms), (false, 2_105));
        assert!(p.art.is_some(), "the same track keeps its artwork");
        n.apply(RESUMED);
        assert!(n.player(0.0).unwrap().playing);
        n.apply(NEXT);
        assert!(
            n.player(0.0).unwrap().art.is_none(),
            "a new track has its own"
        );
    }

    #[test]
    fn nothing_playing_is_no_card() {
        let mut n = NowPlaying::default();
        n.apply(FULL);
        assert!(n.apply(NOTHING));
        assert!(n.player(0.0).is_none());
        assert!(!n.apply("not json"));
    }

    /// The real helper (`scripts/mac-app.sh` first), while something plays:
    /// `cargo test -p wado-host-macos live_now_playing -- --ignored --nocapture`.
    #[test]
    #[ignore = "runs the helper; something must be playing"]
    fn live_now_playing() {
        let lib = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../target/release")
            .join(LIB);
        let a = Adapter::at(&lib).expect("scripts/mac-app.sh");
        let (tx, rx) = std::sync::mpsc::channel();
        let _s = a.stream(move |n| drop(tx.send(n.player(epoch_secs()))));
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
    fn modes_and_a_missing_key() {
        let mut n = NowPlaying::default();
        n.apply(r#"{"title":"A","bundle":"com.apple.Music","shuffle":3,"repeat":2}"#);
        let p = n.player(0.0).unwrap();
        assert_eq!(
            (p.shuffle, p.loop_status.as_deref()),
            (Some(true), Some("Track"))
        );
        assert_eq!((p.bus.as_str(), p.artist.as_str()), ("com.apple.Music", ""));
    }
}
