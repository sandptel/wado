//! The computer's clipboard as the devices see it: the latest copy, where it came from, and the
//! secrets that must never leave.
//!
//! Every copy passes through [`Hub::note`] once — the desktop's (`sync`, from the host watcher),
//! a wado session's (`sync`, from the compositor), a device's (`serve`, a `ClipPush`). The hub
//! then decides, in one place:
//!
//! - **echo** — the value just seen, coming back from where it was written: stops here. Every
//!   crossing writes a value somewhere that then reports a change of its own (the host watcher
//!   fires on our own `wl-copy`), and this is what makes that a single hop.
//! - **secret** — a password manager marked it: remembered by hash and by the line `cliphist`
//!   would show for it, so it can be kept out of every history and refused if asked for; never
//!   sent to a device, never kept.
//! - **shared** — published to every link's follower ([`subscribe`](Hub::subscribe)), and kept
//!   in wado's own history ([`super::ram`]).

use std::collections::VecDeque;
use std::sync::{Arc, Mutex, OnceLock};

use tokio::sync::watch;
use wado_compositor::clipboard::Clip;

use super::ram::Ram;

/// Where a copy was made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Origin {
    /// The computer's own desktop.
    Desktop,
    /// An app in a wado session.
    Session,
    /// A device sent it (`ClipPush`): its key, and its name.
    Device { key: String, name: String },
}

impl Origin {
    /// As the wire says it (`ClipNow.from`, `ClipEntry.from`).
    pub fn label(&self) -> String {
        match self {
            Origin::Desktop => "desktop".into(),
            Origin::Session => "session".into(),
            Origin::Device { name, .. } => name.clone(),
        }
    }

    /// The device that sent it, if one did.
    pub fn device(&self) -> Option<&str> {
        match self {
            Origin::Device { key, .. } => Some(key),
            _ => None,
        }
    }
}

/// One shared copy.
#[derive(Debug, Clone)]
pub struct Copied {
    pub hash: String,
    pub clip: Clip,
    pub from: Origin,
    pub at_ms: u64,
}

/// What [`Hub::note`] made of a copy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Noted {
    /// New, and published.
    Shared,
    /// The value seen last, again: nothing to do.
    Echo,
    /// A password manager's copy: remembered as one, sent nowhere.
    Secret,
    /// Empty, or too big to keep.
    Ignored,
}

/// How many secrets are remembered. Enough for a session of logging in; they are hashes and
/// short previews, in memory only.
const SECRETS: usize = 64;

struct Secret {
    hash: String,
    /// The line `cliphist list` shows for it, if it is text.
    line: Option<String>,
}

pub struct Hub {
    now: watch::Sender<Option<Arc<Copied>>>,
    /// The hash of the last value noted, secret or not — what an echo is recognised by.
    last: Mutex<String>,
    secrets: Mutex<VecDeque<Secret>>,
    ram: Arc<Ram>,
}

impl Default for Hub {
    fn default() -> Self {
        Self {
            now: watch::channel(None).0,
            last: Mutex::default(),
            secrets: Mutex::default(),
            ram: Arc::new(Ram::default()),
        }
    }
}

/// The daemon's hub.
pub fn get() -> &'static Hub {
    static HUB: OnceLock<Hub> = OnceLock::new();
    HUB.get_or_init(Hub::default)
}

/// The name of `data` on the wire: the first 16 bytes of its SHA-256, as lowercase hex.
pub fn hash(data: &[u8]) -> String {
    let d = ring::digest::digest(&ring::digest::SHA256, data);
    d.as_ref()[..16]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Now, in unix milliseconds.
pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// The line `cliphist list` shows for `text`: its words joined by single spaces, cut at 100
/// characters and an ellipsis after — so a secret can be recognised in a history that only has those.
pub fn cliphist_line(text: &str) -> String {
    let joined = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if joined.chars().count() > 100 {
        let mut cut: String = joined.chars().take(100).collect();
        cut.push('…');
        cut
    } else {
        joined
    }
}

impl Hub {
    /// A copy arrived from `from`. See the module for what each outcome means.
    pub fn note(&self, clip: Clip, from: Origin) -> Noted {
        if clip.data.is_empty() {
            return Noted::Ignored;
        }
        let h = hash(&clip.data);
        {
            let mut last = self.last.lock().unwrap_or_else(|e| e.into_inner());
            if *last == h {
                return Noted::Echo;
            }
            last.clone_from(&h);
        }
        if clip.secret {
            self.remember_secret(&clip, h);
            return Noted::Secret;
        }
        let max = if clip.is_text() {
            wado_protocol::clip::MAX_TEXT
        } else {
            wado_protocol::clip::MAX_IMAGE
        };
        if clip.data.len() > max {
            return Noted::Ignored;
        }
        let copied = Copied {
            hash: h,
            clip: normalised(clip),
            from,
            at_ms: now_ms(),
        };
        self.ram.record(&copied);
        self.now.send_replace(Some(Arc::new(copied)));
        Noted::Shared
    }

    fn remember_secret(&self, clip: &Clip, hash: String) {
        let line = clip
            .is_text()
            .then(|| cliphist_line(&String::from_utf8_lossy(&clip.data)));
        let mut s = self.secrets.lock().unwrap_or_else(|e| e.into_inner());
        s.retain(|x| x.hash != hash);
        s.push_front(Secret { hash, line });
        s.truncate(SECRETS);
    }

    /// Every shared copy from now on. A link that subscribes is not handed what was copied before
    /// it came — an hour-old copy is not news.
    pub fn subscribe(&self) -> watch::Receiver<Option<Arc<Copied>>> {
        self.now.subscribe()
    }

    /// Was the value with this hash a secret?
    pub fn is_secret(&self, hash: &str) -> bool {
        self.secrets
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .any(|s| s.hash == hash)
    }

    /// Is this a `cliphist` line for a secret?
    pub fn hides(&self, line: &str) -> bool {
        self.secrets
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .any(|s| s.line.as_deref() == Some(line))
    }

    /// wado's own history: every shared copy, the last 50 — the history when the computer has no
    /// `cliphist`, and the times and origins of entries when it does.
    pub fn ram(&self) -> Arc<Ram> {
        Arc::clone(&self.ram)
    }
}

/// Text under one name, whatever the app offered it as.
fn normalised(mut clip: Clip) -> Clip {
    if clip.is_text() {
        clip.mime = "text/plain;charset=utf-8".into();
    }
    clip
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(s: &str) -> Clip {
        Clip {
            mime: "text/plain".into(),
            data: s.as_bytes().into(),
            secret: false,
        }
    }

    #[test]
    fn an_echo_stops_at_one_hop() {
        let hub = Hub::default();
        let mut rx = hub.subscribe();
        assert_eq!(hub.note(text("x"), Origin::Session), Noted::Shared);
        assert!(rx.has_changed().unwrap());
        rx.mark_unchanged();
        // Written to the desktop, the desktop's watcher reports it back: the echo.
        assert_eq!(hub.note(text("x"), Origin::Desktop), Noted::Echo);
        assert!(!rx.has_changed().unwrap(), "an echo is not published");
        assert_eq!(hub.note(text("y"), Origin::Desktop), Noted::Shared);
        // The same value again after another is a new copy.
        assert_eq!(hub.note(text("x"), Origin::Desktop), Noted::Shared);
        let now = rx.borrow_and_update().clone().unwrap();
        assert_eq!(now.from, Origin::Desktop);
        assert_eq!(now.clip.mime, "text/plain;charset=utf-8");
        assert_eq!(now.hash, hash(b"x"));
    }

    #[test]
    fn a_device_push_is_not_echoed_by_the_desktop() {
        let hub = Hub::default();
        let phone = Origin::Device {
            key: "k".into(),
            name: "Pixel".into(),
        };
        assert_eq!(hub.note(text("from phone"), phone.clone()), Noted::Shared);
        assert_eq!(hub.note(text("from phone"), Origin::Desktop), Noted::Echo);
        assert_eq!(hub.subscribe().borrow().as_ref().unwrap().from, phone);
    }

    #[test]
    fn secrets_are_remembered_and_never_published() {
        let hub = Hub::default();
        let rx = hub.subscribe();
        let mut pw = text("hunter2   hunter2");
        pw.secret = true;
        assert_eq!(hub.note(pw.clone(), Origin::Desktop), Noted::Secret);
        assert!(!rx.has_changed().unwrap());
        assert!(hub.is_secret(&hash(b"hunter2   hunter2")));
        assert!(hub.hides("hunter2 hunter2"), "cliphist's line for it");
        assert!(!hub.hides("hunter2"));
        assert!(hub.ram().list_now().is_empty(), "never kept");
        // The session's watcher seeing it again is an echo, still not published.
        assert_eq!(hub.note(pw, Origin::Session), Noted::Echo);
    }

    #[test]
    fn too_big_is_not_shared() {
        let hub = Hub::default();
        let big = text(&"a".repeat(wado_protocol::clip::MAX_TEXT + 1));
        assert_eq!(hub.note(big, Origin::Desktop), Noted::Ignored);
        assert!(hub.subscribe().borrow().is_none());
        assert_eq!(hub.note(text(""), Origin::Desktop), Noted::Ignored);
    }

    #[test]
    fn cliphist_lines() {
        assert_eq!(cliphist_line("  a\n\tb  c "), "a b c");
        let long = "x".repeat(150);
        let line = cliphist_line(&long);
        assert_eq!(line.chars().count(), 101);
        assert!(line.ends_with('…'));
    }
}
