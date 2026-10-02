//! Who may join this computer — the trust list, and approvals between the devices on it.
//!
//! The relay holds every join until this daemon answers `peer_accept` or `peer_reject`
//! (`WADO_PLAN.md`, Decision Log `2026-10-02`). The rule, decided by the user:
//!
//! - a device whose `client_key` is on the trust list gets straight in;
//! - while the trust list is **empty**, the first device to connect is trusted (there is no one
//!   yet to ask);
//! - any other device **waits** until a device already connected to this computer approves it
//!   — "once", "always" (it joins the trust list) or "deny".
//!
//! **Why files.** A pool hands a new device a *free* daemon, which by definition has no viewer
//! to ask. The approver is connected to a *different* daemon of the same pool, and the relay is
//! deliberately not the place to route policy. Every daemon of a pool runs on one machine and
//! shares [`crate::remote_id::config_dir`], so a pending request is a file there that every
//! daemon can see and show to its own viewer, and the answer is a file next to it.
//!
//! ```text
//!   <config>/wado/trusted_clients        <client_key>\t<name>   one per line
//!   <config>/wado/pending/<id>.json      a device waiting for approval
//!   <config>/wado/pending/<id>.verdict   once | always | deny
//! ```
//!
//! ponytail: polled every 500 ms rather than watched with inotify — a prompt that appears half a
//! second late costs nothing, and polling a tiny directory costs less than the dependency.

use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};
use tracing::{info, warn};

/// How long a request may wait for an answer. Matches the relay's own `APPROVAL_WAIT`.
pub const APPROVAL_WAIT: Duration = Duration::from_secs(600);
const POLL: Duration = Duration::from_millis(500);

/// A device waiting for approval, as other devices are shown it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Request {
    pub id: String,
    pub name: String,
    pub addr: String,
}

#[derive(Debug, PartialEq)]
pub enum Decision {
    /// On the trust list.
    Trusted,
    /// The trust list was empty; this device is now its first entry.
    FirstDevice,
    /// Must be approved by a connected device.
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Verdict {
    Once,
    Always,
    Deny,
}

impl Verdict {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim() {
            "once" => Some(Self::Once),
            "always" => Some(Self::Always),
            "deny" => Some(Self::Deny),
            _ => None,
        }
    }
    fn as_str(self) -> &'static str {
        match self {
            Self::Once => "once",
            Self::Always => "always",
            Self::Deny => "deny",
        }
    }
}

/// The trust list and the pending requests of one machine.
#[derive(Clone)]
pub struct Gate {
    dir: PathBuf,
}

impl Default for Gate {
    fn default() -> Self {
        Self::at(crate::remote_id::config_dir())
    }
}

impl Gate {
    pub fn at(dir: PathBuf) -> Self {
        Self { dir }
    }

    fn trusted_path(&self) -> PathBuf {
        self.dir.join("trusted_clients")
    }
    fn pending_dir(&self) -> PathBuf {
        self.dir.join("pending")
    }
    /// Only ids we minted (the relay's uuid room ids) become file names.
    fn request_path(&self, id: &str, ext: &str) -> Option<PathBuf> {
        let safe = !id.is_empty()
            && id.len() <= 64
            && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-');
        safe.then(|| self.pending_dir().join(format!("{id}.{ext}")))
    }

    fn trusted_keys(&self) -> Vec<String> {
        fs::read_to_string(self.trusted_path())
            .unwrap_or_default()
            .lines()
            .filter_map(|l| l.split('\t').next())
            .map(str::trim)
            .filter(|k| !k.is_empty())
            .map(String::from)
            .collect()
    }

    /// May this device in without asking anyone?
    ///
    /// A client with no key (one older than keys) is never trusted and never becomes the first
    /// device: there is nothing to remember it by, and an empty key accepted as trusted would be
    /// a way around the gate for anyone who simply leaves it off.
    pub fn decide(&self, key: &str, name: &str) -> Decision {
        let keys = self.trusted_keys();
        if !key.is_empty() && keys.iter().any(|k| k == key) {
            return Decision::Trusted;
        }
        if !key.is_empty() && keys.is_empty() {
            self.trust(key, name);
            return Decision::FirstDevice;
        }
        Decision::Unknown
    }

    /// Add a device to the trust list.
    pub fn trust(&self, key: &str, name: &str) {
        if key.is_empty() || self.trusted_keys().iter().any(|k| k == key) {
            return;
        }
        let line = format!("{key}\t{}\n", name.replace(['\t', '\n'], " "));
        let res = fs::create_dir_all(&self.dir).and_then(|_| {
            fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(self.trusted_path())?
                .write_all(line.as_bytes())
        });
        match res {
            Ok(()) => info!("gate: trusted a new device — {name}"),
            Err(e) => warn!("gate: could not write the trust list: {e}"),
        }
    }

    /// Post a request for the pool's connected devices to answer.
    pub fn post(&self, req: &Request) -> bool {
        let Some(path) = self.request_path(&req.id, "json") else {
            return false;
        };
        let res = fs::create_dir_all(self.pending_dir())
            .and_then(|_| fs::write(&path, serde_json::to_vec(req).unwrap_or_default()));
        if let Err(e) = &res {
            warn!("gate: could not post an approval request: {e}");
        }
        res.is_ok()
    }

    /// Withdraw a request and any answer to it.
    pub fn withdraw(&self, id: &str) {
        for ext in ["json", "verdict"] {
            if let Some(p) = self.request_path(id, ext) {
                let _ = fs::remove_file(p);
            }
        }
    }

    /// Answer a request. Ignored if it is no longer pending — answered elsewhere, or withdrawn.
    pub fn answer(&self, id: &str, verdict: Verdict) -> bool {
        let (Some(req), Some(out)) = (
            self.request_path(id, "json"),
            self.request_path(id, "verdict"),
        ) else {
            return false;
        };
        req.exists() && fs::write(out, verdict.as_str()).is_ok()
    }

    /// The answer to a request, once there is one.
    pub fn verdict(&self, id: &str) -> Option<Verdict> {
        let p = self.request_path(id, "verdict")?;
        Verdict::parse(&fs::read_to_string(p).ok()?)
    }

    /// Wait for a request's answer. `None` when it is withdrawn under us or times out.
    pub async fn wait(&self, id: &str, still_wanted: impl Fn() -> bool) -> Option<Verdict> {
        let deadline = tokio::time::Instant::now() + APPROVAL_WAIT;
        while tokio::time::Instant::now() < deadline && still_wanted() {
            if let Some(v) = self.verdict(id) {
                return Some(v);
            }
            tokio::time::sleep(POLL).await;
        }
        None
    }

    /// Every request still waiting. Requests older than [`APPROVAL_WAIT`] are left over from a
    /// daemon that died mid-wait, and are swept here.
    pub fn pending(&self) -> Vec<Request> {
        let Ok(dir) = fs::read_dir(self.pending_dir()) else {
            return Vec::new();
        };
        let now = SystemTime::now();
        let mut out = Vec::new();
        for e in dir.flatten() {
            let path = e.path();
            if path.extension().and_then(|x| x.to_str()) != Some("json") {
                continue;
            }
            let age = e
                .metadata()
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| now.duration_since(t).ok())
                .unwrap_or_default();
            let req = fs::read(&path)
                .ok()
                .and_then(|b| serde_json::from_slice::<Request>(&b).ok());
            match req {
                Some(r) if age < APPROVAL_WAIT => {
                    // Already answered: no longer something to ask about.
                    if self.verdict(&r.id).is_none() {
                        out.push(r);
                    }
                }
                Some(r) => self.withdraw(&r.id),
                None => {
                    let _ = fs::remove_file(&path);
                }
            }
        }
        out.sort_by(|a, b| a.id.cmp(&b.id));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::{Decision, Gate, Request, Verdict};

    fn temp() -> Gate {
        let dir = std::env::temp_dir().join(format!("wado-gate-{}", rand::random::<u64>()));
        Gate::at(dir)
    }

    #[test]
    fn first_device_is_trusted_then_others_must_ask() {
        let g = temp();
        assert_eq!(g.decide("phone", "Phone"), Decision::FirstDevice);
        assert_eq!(g.decide("phone", "Phone"), Decision::Trusted);
        assert_eq!(g.decide("laptop", "Laptop"), Decision::Unknown);
        // No key is never trusted, even into an empty list.
        assert_eq!(temp().decide("", "old client"), Decision::Unknown);
    }

    #[test]
    fn a_request_is_seen_answered_and_cleared() {
        let g = temp();
        let r = Request {
            id: "0b1c-room".into(),
            name: "Laptop".into(),
            addr: "1.2.3.4".into(),
        };
        assert!(g.post(&r));
        assert_eq!(g.pending(), vec![r.clone()]);
        assert!(g.answer(&r.id, Verdict::Always));
        assert_eq!(g.verdict(&r.id), Some(Verdict::Always));
        assert!(
            g.pending().is_empty(),
            "an answered request is not shown again"
        );
        g.withdraw(&r.id);
        assert!(
            !g.answer(&r.id, Verdict::Deny),
            "a withdrawn request cannot be answered"
        );
    }

    #[test]
    fn ids_cannot_escape_the_pending_dir() {
        let g = temp();
        assert!(!g.answer("../trusted_clients", Verdict::Always));
        assert!(!g.post(&Request {
            id: "../x".into(),
            name: String::new(),
            addr: String::new()
        }));
    }
}
