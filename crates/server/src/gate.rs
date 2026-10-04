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
//!   <config>/wado/trusted_clients        <client_key>\t<name>[\t<device pk>\t<pinned>[\t<files>]]
//!   <config>/wado/pending/<id>.json      a device waiting for approval
//!   <config>/wado/pending/<id>.verdict   once | always | deny
//! ```
//!
//! **Since the envelope (`crate::e2e`, `2026-10-04`)** the relay's `client_key` is only a claim:
//! [`Gate::decide`] uses it to park or let in a join, and [`Gate::admit`] makes the decision that
//! counts, once the device has proven its key. A line without a key (older than the envelope,
//! or trusted a moment ago) takes the first key proven for it; `pinned` is 1 when that key was
//! proven with a QR pairing code, which file access requires. `files` is that device's grant —
//! `none` (the default, also when absent), `ro` or `rw` — set by `wado files grant`.
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
/// How long a QR pairing code works.
const PAIR_TTL: Duration = Duration::from_secs(24 * 3600);

fn now_s() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

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

    /// `(key, name)` for every trusted device, oldest first.
    pub fn trusted(&self) -> Vec<(String, String)> {
        fs::read_to_string(self.trusted_path())
            .unwrap_or_default()
            .lines()
            .filter_map(|l| {
                let mut it = l.split('\t');
                let key = it.next()?.trim();
                (!key.is_empty())
                    .then(|| (key.to_string(), it.next().unwrap_or("").trim().to_string()))
            })
            .collect()
    }

    /// The first device ever trusted — the owner unless `security { owner }` names another.
    pub fn first_trusted(&self) -> Option<String> {
        self.trusted().into_iter().next().map(|(k, _)| k)
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
        if !key.is_empty()
            && keys.is_empty()
            && wado_config::live::current().security.trust_first_device
        {
            self.trust(key, name);
            return Decision::FirstDevice;
        }
        Decision::Unknown
    }

    /// A new pairing code for a connect link: single use, good for a day. The QR it goes into is
    /// shown only on this computer, so a device presenting it was shown the host's screen.
    pub fn mint_pair(&self) -> String {
        use rand::Rng;
        let code: String = rand::thread_rng()
            .sample_iter(&rand::distributions::Alphanumeric)
            .take(24)
            .map(char::from)
            .collect();
        let until = now_s() + PAIR_TTL.as_secs();
        let mut lines: Vec<String> = self
            .pairs()
            .into_iter()
            .filter(|(_, t)| *t > now_s())
            .map(|(c, t)| format!("{c}\t{t}"))
            .collect();
        lines.push(format!("{code}\t{until}"));
        if let Err(e) = fs::create_dir_all(&self.dir)
            .and_then(|_| fs::write(self.pairs_path(), lines.join("\n") + "\n"))
        {
            warn!("gate: could not save a pairing code: {e}");
        }
        code
    }

    /// Use up a pairing code: true once for a live code, false for anything else.
    pub fn redeem(&self, code: &str) -> bool {
        if code.is_empty() {
            return false;
        }
        let all = self.pairs();
        let ok = all.iter().any(|(c, t)| c == code && *t > now_s());
        if ok {
            let rest: Vec<String> = all
                .into_iter()
                .filter(|(c, t)| c != code && *t > now_s())
                .map(|(c, t)| format!("{c}\t{t}"))
                .collect();
            let _ = fs::write(self.pairs_path(), rest.join("\n") + "\n");
        }
        ok
    }

    fn pairs_path(&self) -> PathBuf {
        self.dir.join("pair_codes")
    }

    fn pairs(&self) -> Vec<(String, u64)> {
        fs::read_to_string(self.pairs_path())
            .unwrap_or_default()
            .lines()
            .filter_map(|l| {
                let (c, t) = l.split_once('\t')?;
                Some((c.to_string(), t.parse().ok()?))
            })
            .collect()
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

    /// The pairing codes still live, for checking a device's proof against.
    pub fn live_pairs(&self) -> Vec<String> {
        self.pairs()
            .into_iter()
            .filter(|(_, t)| *t > now_s())
            .map(|(c, _)| c)
            .collect()
    }

    /// `(device pk, pinned)` on file for `key`; the pk is empty for a line from before keys.
    pub fn entry(&self, key: &str) -> Option<(String, bool)> {
        fs::read_to_string(self.trusted_path())
            .unwrap_or_default()
            .lines()
            .map(|l| l.split('\t').collect::<Vec<_>>())
            .find(|f| f[0].trim() == key)
            .map(|f| {
                let pk = f.get(2).map_or("", |s| s.trim()).to_string();
                (pk, f.get(3).is_some_and(|s| s.trim() == "1"))
            })
    }

    /// Write `key`'s line with its proven key, in place or appended. Its file grant is kept.
    fn set(&self, key: &str, name: &str, pk: &str, pinned: bool) {
        let files = self.files_level(key);
        self.write_line(key, name, pk, pinned, &files);
    }

    fn write_line(&self, key: &str, name: &str, pk: &str, pinned: bool, files: &str) {
        let name = name.replace(['\t', '\n'], " ");
        let line = format!("{key}\t{name}\t{pk}\t{}\t{files}", u8::from(pinned));
        let mut lines: Vec<String> = fs::read_to_string(self.trusted_path())
            .unwrap_or_default()
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(String::from)
            .collect();
        match lines.iter_mut().find(|l| l.split('\t').next() == Some(key)) {
            Some(l) => *l = line,
            None => lines.push(line),
        }
        if let Err(e) = fs::create_dir_all(&self.dir)
            .and_then(|_| fs::write(self.trusted_path(), lines.join("\n") + "\n"))
        {
            warn!("gate: could not write the trust list: {e}");
        }
    }

    /// Every trusted device: `(key, name, pinned, files level)`, oldest first.
    pub fn devices(&self) -> Vec<(String, String, bool, String)> {
        fs::read_to_string(self.trusted_path())
            .unwrap_or_default()
            .lines()
            .map(|l| l.split('\t').map(str::trim).collect::<Vec<_>>())
            .filter(|f| !f[0].is_empty())
            .map(|f| {
                let get = |i: usize| f.get(i).copied().unwrap_or("");
                let lvl = match get(4) {
                    "ro" | "rw" => get(4),
                    _ => "none",
                };
                (get(0).into(), get(1).into(), get(3) == "1", lvl.into())
            })
            .collect()
    }

    fn files_level(&self, key: &str) -> String {
        self.devices()
            .into_iter()
            .find(|d| d.0 == key)
            .map_or_else(|| "none".into(), |d| d.3)
    }

    /// What `key` may do with files: `none`, `ro` or `rw`. A grant counts only on a line whose
    /// key was proven with a QR code — a device trusted on first use, or a legacy line, gets
    /// nothing until it is re-paired.
    pub fn files_access(&self, key: &str) -> &'static str {
        match self.devices().into_iter().find(|d| d.0 == key) {
            Some((_, _, true, l)) if l == "rw" => "rw",
            Some((_, _, true, l)) if l == "ro" => "ro",
            _ => "none",
        }
    }

    /// Set a device's file grant. `who` is its key, a prefix of it, or its exact name; it must
    /// name exactly one device. Returns the device's name.
    pub fn grant(&self, who: &str, level: &str) -> Result<String, String> {
        if !matches!(level, "none" | "ro" | "rw") {
            return Err(format!("`{level}` is not a level — use none, ro or rw"));
        }
        let all = self.devices();
        let hits: Vec<_> = all
            .iter()
            .filter(|d| !who.is_empty() && (d.0.starts_with(who) || d.1 == who))
            .collect();
        let (key, name, pinned, _) = match hits.as_slice() {
            [one] => (*one).clone(),
            [] => return Err(format!("no trusted device matches `{who}`")),
            _ => {
                return Err(format!(
                    "`{who}` matches {} devices — use more of the key",
                    hits.len()
                ));
            }
        };
        let pk = self.entry(&key).map(|e| e.0).unwrap_or_default();
        self.write_line(&key, &name, &pk, pinned, level);
        info!("gate: files access for {name} set to {level}");
        Ok(name)
    }

    /// The decision that counts: may the device that proved `pk` in, as `key`?
    ///
    /// `once` — it was let in for this visit only. `pair` — the code it proved it holds.
    pub fn admit(
        &self,
        key: &str,
        name: &str,
        pk: &str,
        once: bool,
        pair: Option<&str>,
    ) -> Result<(), &'static str> {
        // A QR code is the owner's say-so, and it pins whatever key proved it — a re-pair is
        // how a device that lost its key comes back.
        if pair.is_some_and(|c| self.redeem(c)) {
            info!("gate: {name} paired with a QR code — trusted and pinned");
            self.set(key, name, pk, true);
            return Ok(());
        }
        match self.entry(key) {
            Some((on_file, _)) if on_file == pk => Ok(()),
            Some((on_file, _)) if !on_file.is_empty() => {
                Err("this device's key is not the one this computer trusts for it")
            }
            Some(_) => {
                info!("gate: {name} proved a key for the first time — remembered (not pinned)");
                self.set(key, name, pk, false);
                Ok(())
            }
            None if once => Ok(()),
            None => Err("this device is not trusted here — scan the computer's QR code again"),
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
    fn a_pairing_code_works_once() {
        let g = temp();
        let c = g.mint_pair();
        assert!(!g.redeem("nope"));
        assert!(!g.redeem(""));
        assert!(g.redeem(&c));
        assert!(!g.redeem(&c), "a code must be single use");
    }

    #[test]
    fn the_proven_key_decides_not_the_claimed_id() {
        let g = temp();
        // A line from before keys takes the first key proven for it…
        g.trust("phone", "Phone");
        assert!(g.admit("phone", "Phone", "PK1", false, None).is_ok());
        assert!(g.admit("phone", "Phone", "PK1", false, None).is_ok());
        // …and after that a relay naming "phone" with any other key gets nothing.
        assert!(g.admit("phone", "Phone", "EVIL", false, None).is_err());
        // Unknown and not "once": refused, even though the relay let it in on a pair claim.
        assert!(g.admit("laptop", "Laptop", "PK2", false, None).is_err());
        assert!(g.admit("laptop", "Laptop", "PK2", true, None).is_ok());
        assert_eq!(g.entry("laptop"), None, "once is not remembered");
        // A proven QR code trusts and pins, and replaces a lost key.
        let c = g.mint_pair();
        assert!(g.admit("phone", "Phone", "PK3", false, Some(&c)).is_ok());
        assert_eq!(g.entry("phone"), Some(("PK3".into(), true)));
        assert!(
            g.admit("phone", "Phone", "PK1", false, Some(&c)).is_err(),
            "single use"
        );
        assert_eq!(g.trusted().len(), 1, "rewritten in place, not appended");
    }

    #[test]
    fn file_access_needs_a_grant_and_a_qr_pinned_key() {
        let g = temp();
        g.trust("phone", "Phone");
        assert!(g.admit("phone", "Phone", "PK1", false, None).is_ok());
        assert_eq!(g.files_access("phone"), "none", "no grant yet");
        assert_eq!(g.grant("pho", "rw").unwrap(), "Phone");
        assert_eq!(
            g.files_access("phone"),
            "none",
            "trusted on first use, not pinned"
        );
        let c = g.mint_pair();
        assert!(g.admit("phone", "Phone", "PK1", false, Some(&c)).is_ok());
        assert_eq!(g.files_access("phone"), "rw", "a re-pair keeps the grant");
        assert!(g.grant("phone", "root").is_err());
        assert!(g.grant("nobody", "ro").is_err());
        g.trust("phone2", "Phone 2");
        assert!(
            g.grant("phone", "ro").is_err(),
            "a prefix of two keys is ambiguous"
        );
        assert!(g.grant("Phone 2", "ro").is_ok(), "by exact name");
        assert_eq!(g.files_access("laptop"), "none");
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
