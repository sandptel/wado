//! Unlocking without the passkey: the device asks, and someone **at the computer** says yes
//! (Decision Log `2026-10-30`, "a passkey must never trap the person"). For the phone whose
//! fingerprint prompt hangs, whose screen lock changed, whose passkey was made in another app.
//!
//! A yes opens the same window a passkey would ([`super::unlock::open_approved`]: the device's
//! own lifetime rules). Nobody answering within [`WAIT`] is a no. Each device may ask
//! [`PER_DEVICE`] times in [`PER`]; one request at a time — asking again while one waits finds
//! the same one. Every step is logged.
//!
//! The daemon holding the link, `wado passkey approve` and the `wado tui` panel are separate
//! processes of one machine, so a request is a file, like a typed pairing's check words
//! (`crate::gate::confirm`):
//!
//! ```text
//!   <config>/wado/passkey_approve/<id>   <check>\t<device key>\t<device name>\t<since>
//! ```
//!
//! The daemon writes `ask`, a screen answers `yes` or `no`, and the daemon settles it —
//! `approved`, `declined`, `expired`, `left` or `cancelled` — so every screen can say how it ended.
//!
//! ponytail: polled, like the gate's other files — a person answers in seconds.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use tokio::sync::mpsc;
use tracing::{info, warn};
use wado_protocol::passkey::{
    APPROVE_APPROVED, APPROVE_CANCELLED, APPROVE_DECLINED, APPROVE_EXPIRED, APPROVE_REFUSED,
    APPROVE_WAITING,
};
use wado_protocol::relay::RelayMsg;

use super::{Store, now_s, unlock};

/// How long someone at the computer has to answer.
pub const WAIT: Duration = Duration::from_secs(120);
/// Requests per device per [`PER`].
pub const PER_DEVICE: usize = 3;
pub const PER: Duration = Duration::from_secs(10 * 60);
const POLL: Duration = Duration::from_millis(300);

/// Where a request stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Check {
    /// Waiting for someone at the computer.
    Asking,
    /// Answered at the computer; the daemon has not acted on it yet.
    Yes,
    No,
    /// Settled by the daemon.
    Approved,
    Declined,
    Expired,
    /// The device went away before anyone answered.
    Left,
    /// The device withdrew it.
    Cancelled,
}

impl Check {
    fn as_str(self) -> &'static str {
        match self {
            Self::Asking => "ask",
            Self::Yes => "yes",
            Self::No => "no",
            Self::Approved => "approved",
            Self::Declined => "declined",
            Self::Expired => "expired",
            Self::Left => "left",
            Self::Cancelled => "cancelled",
        }
    }
    fn parse(s: &str) -> Option<Self> {
        [
            Self::Asking,
            Self::Yes,
            Self::No,
            Self::Approved,
            Self::Declined,
            Self::Expired,
            Self::Left,
            Self::Cancelled,
        ]
        .into_iter()
        .find(|c| c.as_str() == s)
    }
}

/// One request, as a screen shows it.
#[derive(Debug, Clone, PartialEq)]
pub struct Ask {
    pub id: String,
    pub key: String,
    pub device: String,
    /// Unix seconds it was asked.
    pub since: u64,
    pub check: Check,
}

impl Ask {
    /// Seconds left to answer.
    pub fn left(&self) -> u64 {
        (self.since + WAIT.as_secs()).saturating_sub(now_s())
    }
}

/// The requests folder of one config directory.
#[derive(Clone)]
pub struct Approvals {
    dir: PathBuf,
}

impl Default for Approvals {
    fn default() -> Self {
        Self::of(&Store::default())
    }
}

impl Approvals {
    /// Beside the passkeys folder of `store` (never inside it: a file there is a device key).
    pub fn of(store: &Store) -> Self {
        let base = store.dir().parent().map(PathBuf::from).unwrap_or_default();
        Self {
            dir: base.join("passkey_approve"),
        }
    }

    /// Only a short hex id names a file.
    fn path(&self, id: &str) -> Option<PathBuf> {
        (id.len() == 16 && id.chars().all(|c| c.is_ascii_hexdigit())).then(|| self.dir.join(id))
    }

    fn write(&self, a: &Ask) -> bool {
        let Some(path) = self.path(&a.id) else {
            return false;
        };
        let clean = |s: &str| s.replace(['\t', '\n'], " ");
        let body = format!(
            "{}\t{}\t{}\t{}\n",
            a.check.as_str(),
            clean(&a.key),
            clean(&a.device),
            a.since
        );
        // Written whole and renamed in, so a screen never reads half an answer.
        let tmp = path.with_extension("tmp");
        let res = fs::create_dir_all(&self.dir)
            .and_then(|_| fs::write(&tmp, body))
            .and_then(|_| fs::rename(&tmp, &path));
        if let Err(e) = &res {
            warn!("passkey: could not write an approval request: {e}");
        }
        res.is_ok()
    }

    pub fn get(&self, id: &str) -> Option<Ask> {
        let text = fs::read_to_string(self.path(id)?).ok()?;
        let mut f = text.trim_end_matches('\n').split('\t');
        Some(Ask {
            check: Check::parse(f.next()?)?,
            key: f.next()?.into(),
            device: f.next()?.into(),
            since: f.next()?.parse().ok()?,
            id: id.into(),
        })
    }

    /// Every request still waiting for an answer, oldest first.
    pub fn asking(&self) -> Vec<Ask> {
        let Ok(dir) = fs::read_dir(&self.dir) else {
            return Vec::new();
        };
        let mut out: Vec<Ask> = dir
            .flatten()
            .filter_map(|e| self.get(e.file_name().to_str()?))
            .filter(|a| a.check == Check::Asking && a.left() > 0)
            .collect();
        out.sort_by_key(|a| a.since);
        out
    }

    /// Put `key`'s request to whoever is at the computer — or the one it already has waiting.
    pub fn ask(&self, key: &str, device: &str) -> Option<Ask> {
        self.sweep();
        if let Some(a) = self.asking().into_iter().find(|a| a.key == key) {
            return Some(a);
        }
        let a = Ask {
            id: format!("{:016x}", rand::random::<u64>()),
            key: key.into(),
            device: device.into(),
            since: now_s(),
            check: Check::Asking,
        };
        self.write(&a).then_some(a)
    }

    /// Answer at the computer. False unless the request is still waiting.
    pub fn answer(&self, id: &str, yes: bool) -> bool {
        match self.get(id) {
            Some(a) if a.check == Check::Asking && a.left() > 0 => self.write(&Ask {
                check: if yes { Check::Yes } else { Check::No },
                ..a
            }),
            _ => false,
        }
    }

    /// The daemon's last word on a request, for the screens.
    pub fn settle(&self, id: &str, check: Check) {
        if let Some(a) = self.get(id) {
            self.write(&Ask { check, ..a });
        }
    }

    /// The device withdrew everything it had waiting.
    pub fn cancel(&self, key: &str) -> bool {
        let mine: Vec<Ask> = self.asking().into_iter().filter(|a| a.key == key).collect();
        for a in &mine {
            self.settle(&a.id, Check::Cancelled);
        }
        !mine.is_empty()
    }

    /// Wait for the answer to `id`: [`Check::Yes`], [`Check::No`], [`Check::Cancelled`],
    /// [`Check::Expired`] after `wait`, or [`Check::Left`] once `still_wanted` turns false.
    pub async fn await_answer(
        &self,
        id: &str,
        wait: Duration,
        still_wanted: impl Fn() -> bool,
    ) -> Check {
        let deadline = tokio::time::Instant::now() + wait;
        loop {
            match self.get(id).map(|a| a.check) {
                Some(c @ (Check::Yes | Check::No | Check::Cancelled)) => return c,
                // Withdrawn under us: nobody can answer it any more.
                None => return Check::Expired,
                _ => {}
            }
            if !still_wanted() {
                return Check::Left;
            }
            if tokio::time::Instant::now() >= deadline {
                return Check::Expired;
            }
            tokio::time::sleep(POLL).await;
        }
    }

    /// Requests from long ago.
    fn sweep(&self) {
        let Ok(dir) = fs::read_dir(&self.dir) else {
            return;
        };
        for e in dir.flatten() {
            let stale = e
                .file_name()
                .to_str()
                .and_then(|t| self.get(t))
                .is_none_or(|a| now_s().saturating_sub(a.since) > 2 * WAIT.as_secs());
            if stale {
                let _ = fs::remove_file(e.path());
            }
        }
    }
}

/// How often each device has asked lately.
#[derive(Default)]
pub struct Limiter {
    asked: HashMap<String, Vec<Instant>>,
}

impl Limiter {
    /// May `key` ask once more now? Counts the ask when it may.
    pub fn allow(&mut self, key: &str, now: Instant) -> bool {
        let times = self.asked.entry(key.to_string()).or_default();
        times.retain(|t| now.saturating_duration_since(*t) < PER);
        if times.len() >= PER_DEVICE {
            return false;
        }
        times.push(now);
        true
    }
}

fn limiter() -> std::sync::MutexGuard<'static, Limiter> {
    static L: std::sync::LazyLock<Mutex<Limiter>> = std::sync::LazyLock::new(Mutex::default);
    L.lock().unwrap_or_else(|e| e.into_inner())
}

/// Requests a task of this daemon is already waiting on, so a second ask finds it.
fn waiting() -> std::sync::MutexGuard<'static, HashSet<String>> {
    static W: std::sync::LazyLock<Mutex<HashSet<String>>> =
        std::sync::LazyLock::new(Mutex::default);
    W.lock().unwrap_or_else(|e| e.into_inner())
}

async fn say(tx: &mpsc::Sender<String>, state: &str, left_s: u64, message: &str) {
    let m = RelayMsg::PasskeyApproveState {
        state: state.into(),
        left_s: left_s.min(u64::from(u32::MAX)) as u32,
        message: message.into(),
    };
    crate::relay_client::send_relay(tx, &m).await.ok();
}

const TOO_OFTEN: &str =
    "This phone asked the computer too often — wait a few minutes, or use your passkey.";
const NOT_SAVED: &str = "The computer could not record the request — try again.";
const DECLINED: &str = "Turned down on the computer.";
const EXPIRED: &str =
    "Nobody answered on the computer in 2 minutes — try again when someone is there.";

/// `key` asks to be let in by someone at the computer: put the question, wait for it, and tell
/// the device how it went — answered on `tx`, never dropped. Runs on its own task, so the link
/// keeps flowing while a person walks to the computer.
pub fn start(store: Store, key: String, name: String, tx: mpsc::Sender<String>) {
    tokio::spawn(async move { run(&store, &key, &name, &tx, WAIT).await });
}

pub(crate) async fn run(
    store: &Store,
    key: &str,
    name: &str,
    tx: &mpsc::Sender<String>,
    wait: Duration,
) {
    // Already open: nothing to ask.
    if unlock::left_in(store, key).is_some() || !store.required(key) {
        say(tx, APPROVE_APPROVED, 0, "").await;
        return;
    }
    let approvals = Approvals::of(store);
    let pending = approvals.asking().into_iter().find(|a| a.key == key);
    if pending.is_none() && !limiter().allow(key, Instant::now()) {
        warn!(
            device = name,
            "passkey: approval not asked — too many requests"
        );
        say(tx, APPROVE_REFUSED, 0, TOO_OFTEN).await;
        return;
    }
    let Some(ask) = pending.or_else(|| approvals.ask(key, name)) else {
        say(tx, APPROVE_REFUSED, 0, NOT_SAVED).await;
        return;
    };
    say(tx, APPROVE_WAITING, ask.left(), "").await;
    // A second ask for the same request (a double tap, a reconnect): the first task answers.
    if !waiting().insert(ask.id.clone()) {
        return;
    }
    info!(
        device = name,
        "passkey: asked to be approved on the computer — `wado passkey approve` or `wado tui`"
    );
    super::approve_notify::show(name);
    let wait = Duration::from_secs(ask.left()).min(wait);
    let check = approvals
        .await_answer(&ask.id, wait, || !tx.is_closed())
        .await;
    waiting().remove(&ask.id);
    match check {
        Check::Yes => {
            unlock::open_approved(store, key);
            approvals.settle(&ask.id, Check::Approved);
            info!(
                device = name,
                "passkey: approved on the computer — unlocked"
            );
            say(tx, APPROVE_APPROVED, 0, "").await;
            let state = super::relay::state(store, key, "");
            crate::relay_client::send_relay(tx, &RelayMsg::PasskeyState { state })
                .await
                .ok();
        }
        Check::No => {
            approvals.settle(&ask.id, Check::Declined);
            warn!(
                device = name,
                "passkey: approval turned down on the computer"
            );
            say(tx, APPROVE_DECLINED, 0, DECLINED).await;
        }
        Check::Cancelled => {
            info!(
                device = name,
                "passkey: approval request withdrawn by the device"
            );
            say(tx, APPROVE_CANCELLED, 0, "").await;
        }
        Check::Left => {
            approvals.settle(&ask.id, Check::Left);
            info!(
                device = name,
                "passkey: approval request dropped — the device left"
            );
        }
        _ => {
            approvals.settle(&ask.id, Check::Expired);
            warn!(
                device = name,
                "passkey: approval request expired — nobody answered"
            );
            say(tx, APPROVE_EXPIRED, 0, EXPIRED).await;
        }
    }
}

/// The device withdrew its request.
pub async fn cancel(store: &Store, key: &str, name: &str, tx: &mpsc::Sender<String>) {
    // The waiting task says `cancelled` when it sees it; with none waiting, say it here.
    if !Approvals::of(store).cancel(key) {
        say(tx, APPROVE_CANCELLED, 0, "").await;
    } else {
        info!(device = name, "passkey: approval request withdrawn");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp() -> (PathBuf, Store) {
        let dir = std::env::temp_dir().join(format!("wado-pk-approve-{}", rand::random::<u64>()));
        fs::create_dir_all(&dir).unwrap();
        (dir.clone(), Store::at(dir.join("passkeys")))
    }

    #[test]
    fn asked_answered_once_and_settled() {
        let (dir, store) = temp();
        let ap = Approvals::of(&store);
        let a = ap.ask("dev-a", "Android · Chrome").unwrap();
        assert_eq!(ap.asking().len(), 1);
        assert!(a.left() > 110);
        assert_eq!(
            ap.ask("dev-a", "Android · Chrome").unwrap().id,
            a.id,
            "one at a time"
        );
        let b = ap.ask("dev-b", "Pixel").unwrap();
        assert_ne!(a.id, b.id);
        assert!(ap.answer(&a.id, true));
        assert!(!ap.answer(&a.id, false), "answered already");
        assert_eq!(ap.asking().len(), 1, "only b waits");
        ap.settle(&a.id, Check::Approved);
        assert!(!ap.answer(&a.id, true), "settled");
        assert!(!ap.answer("../../trusted_clients", true), "ids only");
        assert!(ap.cancel("dev-b"));
        assert!(ap.asking().is_empty());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn an_old_request_cannot_be_answered() {
        let (dir, store) = temp();
        let ap = Approvals::of(&store);
        let mut a = ap.ask("dev-a", "Phone").unwrap();
        a.since -= WAIT.as_secs() + 1;
        ap.write(&a);
        assert!(ap.asking().is_empty(), "expired: not shown");
        assert!(!ap.answer(&a.id, true), "and a late yes does nothing");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn each_device_asks_a_few_times_per_ten_minutes() {
        let mut l = Limiter::default();
        let t = Instant::now();
        for _ in 0..PER_DEVICE {
            assert!(l.allow("phone", t));
        }
        assert!(!l.allow("phone", t + Duration::from_secs(60)), "too often");
        assert!(l.allow("tablet", t), "another device is not held back");
        assert!(l.allow("phone", t + PER), "ten minutes on, again");
    }

    fn cred() -> super::super::verify::Credential {
        super::super::verify::Credential {
            cred_id: "c".into(),
            key: "k".into(),
            count: 0,
            origin: "https://a.b".into(),
            rp_id: "a.b".into(),
            added: 0,
        }
    }

    async fn next(rx: &mut mpsc::Receiver<String>) -> RelayMsg {
        let t = tokio::time::timeout(Duration::from_secs(5), rx.recv()).await;
        serde_json::from_str(&t.unwrap().unwrap()).unwrap()
    }

    fn approve_state(m: &RelayMsg) -> &str {
        match m {
            RelayMsg::PasskeyApproveState { state, .. } => state,
            _ => "",
        }
    }

    #[tokio::test]
    async fn a_yes_opens_the_window_a_passkey_would() {
        let (dir, store) = temp();
        let key = format!("dev-{}", rand::random::<u64>());
        fs::write(
            dir.join("trusted_clients"),
            format!("{key}\tPhone\tPK\t1\n"),
        )
        .unwrap();
        store.put(&key, &cred()).unwrap();
        // This device chose an hour: an approval gives it an hour, as its passkey would.
        unlock::open_in(&store, &key);
        let mut p = store.prefs(&key);
        p.stay = "1h".into();
        store.set_prefs(&key, &p).unwrap();
        unlock::lock_in(&store, &key);
        assert_eq!(
            super::super::refused_with(&store, &key, true),
            Some(super::super::LOCKED)
        );

        let (tx, mut rx) = mpsc::channel(8);
        let s2 = store.clone();
        let k2 = key.clone();
        let task = tokio::spawn(async move { run(&s2, &k2, "Phone", &tx, WAIT).await });
        assert_eq!(approve_state(&next(&mut rx).await), APPROVE_WAITING);
        let ap = Approvals::of(&store);
        let ask = ap.asking().into_iter().find(|a| a.key == key).unwrap();
        assert!(ap.answer(&ask.id, true));
        assert_eq!(approve_state(&next(&mut rx).await), APPROVE_APPROVED);
        let RelayMsg::PasskeyState { state } = next(&mut rx).await else {
            panic!("the state follows")
        };
        assert!(state.unlocked && state.stay == "1h" && state.idle_s == 3600);
        task.await.unwrap();
        assert_eq!(super::super::refused_with(&store, &key, true), None);
        assert_eq!(ap.get(&ask.id).unwrap().check, Check::Approved);
        unlock::lock_in(&store, &key);
        let _ = fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn no_or_silence_leaves_it_locked() {
        let (dir, store) = temp();
        let key = format!("dev-{}", rand::random::<u64>());
        store.put(&key, &cred()).unwrap();
        let ap = Approvals::of(&store);

        let (tx, mut rx) = mpsc::channel(8);
        let (s2, k2) = (store.clone(), key.clone());
        let t = tokio::spawn(async move { run(&s2, &k2, "Phone", &tx, WAIT).await });
        assert_eq!(approve_state(&next(&mut rx).await), APPROVE_WAITING);
        let ask = ap.asking().into_iter().find(|a| a.key == key).unwrap();
        ap.answer(&ask.id, false);
        let m = next(&mut rx).await;
        assert_eq!(approve_state(&m), APPROVE_DECLINED);
        t.await.unwrap();
        assert!(super::super::refused_with(&store, &key, false).is_some());

        // Nobody answers: expired, after the wait (short here).
        let (tx, mut rx) = mpsc::channel(8);
        let (s2, k2) = (store.clone(), key.clone());
        let t =
            tokio::spawn(
                async move { run(&s2, &k2, "Phone", &tx, Duration::from_millis(700)).await },
            );
        assert_eq!(approve_state(&next(&mut rx).await), APPROVE_WAITING);
        assert_eq!(approve_state(&next(&mut rx).await), APPROVE_EXPIRED);
        t.await.unwrap();
        assert!(super::super::refused_with(&store, &key, false).is_some());

        // The device withdraws it.
        let (tx, mut rx) = mpsc::channel(8);
        let (s2, k2) = (store.clone(), key.clone());
        let t = tokio::spawn(async move { run(&s2, &k2, "Phone", &tx, WAIT).await });
        assert_eq!(approve_state(&next(&mut rx).await), APPROVE_WAITING);
        assert!(ap.cancel(&key));
        assert_eq!(approve_state(&next(&mut rx).await), APPROVE_CANCELLED);
        t.await.unwrap();

        // Three asks in ten minutes: the fourth is refused, and logged as such.
        let (tx, mut rx) = mpsc::channel(8);
        run(&store, &key, "Phone", &tx, Duration::from_millis(1)).await;
        let m = next(&mut rx).await;
        assert_eq!(approve_state(&m), APPROVE_REFUSED, "{m:?}");
        let _ = fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn a_device_with_no_passkey_can_be_let_in_too() {
        let (dir, store) = temp();
        let key = format!("dev-{}", rand::random::<u64>());
        assert_eq!(
            super::super::refused_with(&store, &key, true),
            Some(super::super::NO_PASSKEY)
        );
        let (tx, mut rx) = mpsc::channel(8);
        let (s2, k2) = (store.clone(), key.clone());
        let t = tokio::spawn(async move { run(&s2, &k2, "Phone", &tx, WAIT).await });
        assert_eq!(approve_state(&next(&mut rx).await), APPROVE_WAITING);
        let ap = Approvals::of(&store);
        let ask = ap.asking().into_iter().find(|a| a.key == key).unwrap();
        ap.answer(&ask.id, true);
        assert_eq!(approve_state(&next(&mut rx).await), APPROVE_APPROVED);
        let RelayMsg::PasskeyState { state } = next(&mut rx).await else {
            panic!()
        };
        assert!(state.unlocked && !state.registered && state.idle_s == 900);
        t.await.unwrap();
        assert_eq!(super::super::refused_with(&store, &key, true), None);
        // Bound to its link, like the default window.
        unlock::lock_link_in(&store);
        assert_eq!(
            super::super::refused_with(&store, &key, true),
            Some(super::super::NO_PASSKEY)
        );
        let _ = fs::remove_dir_all(dir);
    }
}
