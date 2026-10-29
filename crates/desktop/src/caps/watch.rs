// SPDX-License-Identifier: AGPL-3.0-only
//! State, pushed: each producer publishes its latest snapshot on a `tokio::sync::watch` channel,
//! the shape the platform layer gives every stateful capability. The UI subscribes
//! (`crate::bridge`) and never polls.
//!
//! Today the producers are pollers over the daemons' files and sockets — those are written by
//! other processes, so there is nothing to subscribe to yet. Each runs on its own thread so a
//! slow relay or a big log never reaches the event loop; a `poke` makes the next round happen
//! now, after an action, instead of at the next tick.

use std::{
    collections::BTreeSet,
    sync::{Arc, Mutex, mpsc},
    time::Duration,
};

use tokio::sync::watch;
use wado_config::Config;

use super::{
    computers::{self, Computer, Seen},
    daemon::{self, Pool},
    devices,
    logs::{Encoder, Line, Tail},
    pairing::{self, Ask, CodeState, Minted},
    sessions::SessionSummary,
};

/// Everything the host console shows, read at one moment.
#[derive(Clone)]
pub struct HostSnapshot {
    pub hostname: String,
    pub pool: Arc<Pool>,
    pub sessions: Vec<SessionSummary>,
    /// The link of whoever watches each session, in the same order.
    pub session_links: Vec<Option<wado_protocol::link::DeviceLink>>,
    pub devices: Vec<devices::Row>,
    pub asks: Vec<Ask>,
    /// What became of the pairing code on screen, if there is one.
    pub pair: Option<(Minted, CodeState)>,
    pub logs: Arc<Vec<Line>>,
    pub logs_generation: u64,
    pub log_path: Option<String>,
    pub encoder: Encoder,
    pub config: Result<Arc<Config>, String>,
    pub overridden: Arc<BTreeSet<String>>,
}

/// A running producer: its channel, and a way to make it look again now.
pub struct Producer<T> {
    pub rx: watch::Receiver<T>,
    poke: mpsc::Sender<()>,
}

impl<T> Producer<T> {
    pub fn poke(&self) {
        let _ = self.poke.send(());
    }
}

/// The pairing code the console is showing, shared with the producer so it can report on it.
pub type ShownCode = Arc<Mutex<Option<Minted>>>;

pub fn host(shown: ShownCode, every: Duration) -> Producer<Option<Arc<HostSnapshot>>> {
    let (tx, rx) = watch::channel(None);
    let (poke, pokes) = mpsc::channel::<()>();
    std::thread::Builder::new()
        .name("wado-desktop-host".into())
        .spawn(move || {
            let mut tail = Tail::default();
            let hostname = daemon::hostname();
            loop {
                let snap = read_host(&hostname, &shown, &mut tail);
                if tx.send(Some(Arc::new(snap))).is_err() {
                    return;
                }
                match pokes.recv_timeout(every) {
                    Ok(()) | Err(mpsc::RecvTimeoutError::Timeout) => {
                        while pokes.try_recv().is_ok() {}
                    }
                    Err(mpsc::RecvTimeoutError::Disconnected) => return,
                }
            }
        })
        .expect("a thread");
    Producer { rx, poke }
}

/// One read, the way `wado tui` takes one each second.
pub fn read_host(hostname: &str, shown: &ShownCode, tail: &mut Tail) -> HostSnapshot {
    let config = super::config::load();
    let (pool, sessions) = daemon::read();
    let devices = devices::list(&sessions);
    let pair = shown.lock().ok().and_then(|m| m.clone()).map(|m| {
        let st = pairing::state(&m);
        (m, st)
    });
    tail.refresh();
    HostSnapshot {
        hostname: hostname.to_string(),
        pool: Arc::new(pool),
        session_links: sessions.iter().map(super::sessions::viewer_link).collect(),
        sessions,
        devices,
        asks: pairing::asking(),
        pair,
        logs: tail.lines.clone(),
        logs_generation: tail.generation,
        log_path: tail.path().map(|p| p.display().to_string()),
        encoder: tail.encoder.clone(),
        config,
        overridden: Arc::new(super::config::overridden()),
    }
}

/// What each saved computer's relay said, by Remote ID.
pub type Sightings = Vec<(String, Seen)>;

/// Ask every computer's relay whether it is up, every `every`, in parallel.
pub fn computers(list: Arc<Mutex<Vec<Computer>>>, every: Duration) -> Producer<Sightings> {
    let (tx, rx) = watch::channel(Vec::new());
    let (poke, pokes) = mpsc::channel::<()>();
    std::thread::Builder::new()
        .name("wado-desktop-relays".into())
        .spawn(move || {
            loop {
                let now: Vec<Computer> = list.lock().map(|l| l.clone()).unwrap_or_default();
                let seen: Sightings = std::thread::scope(|s| {
                    let hs: Vec<_> = now
                        .iter()
                        .map(|c| s.spawn(move || (c.id.clone(), computers::probe(c))))
                        .collect();
                    hs.into_iter().filter_map(|h| h.join().ok()).collect()
                });
                if tx.send(seen).is_err() {
                    return;
                }
                match pokes.recv_timeout(every) {
                    Ok(()) | Err(mpsc::RecvTimeoutError::Timeout) => {
                        while pokes.try_recv().is_ok() {}
                    }
                    Err(mpsc::RecvTimeoutError::Disconnected) => return,
                }
            }
        })
        .expect("a thread");
    Producer { rx, poke }
}
