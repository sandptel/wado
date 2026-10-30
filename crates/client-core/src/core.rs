//! The core, as its UI sees it: commands go in and return at once, state comes out as events
//! ([`Core::next_event`]). It owns a small tokio runtime, this device's identity, the saved
//! computers and their pins, and one control-room link per watched computer.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tokio::runtime::Runtime;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::event::{ComputerView, CoreEvent};
use crate::identity::Identity;
use crate::link::driver::{self, Clock, LinkCmd};
use crate::link::outbox::Class;
use crate::link::state::{Link, LinkCfg, Room};
use crate::pair::flow::{self, PairCtx, PairStep};
use crate::pair::link::{self as pairlink, PairCode, PairLink};
use crate::pins::Pins;
use crate::profiles::Profiles;
use crate::pump::Pump;
use crate::queue::EventQueue;
use crate::relay::transport::{Connector, WsConnector};
use crate::request::{Ops, Outcome};
use crate::secret::SecretBox;

struct Watched {
    cmds: mpsc::UnboundedSender<LinkCmd>,
    tasks: Vec<JoinHandle<()>>,
}

struct Inner {
    identity: Arc<Identity>,
    profiles: Arc<Mutex<Profiles>>,
    pins: Arc<Mutex<Pins>>,
    name: String,
    connector: Arc<dyn Connector>,
    events: Arc<EventQueue>,
    ops: Mutex<Ops>,
    links: Mutex<HashMap<String, Watched>>,
    /// Rooms the UI opened itself (`open_room`), by room id.
    rooms: Mutex<HashMap<String, Watched>>,
    next_room: std::sync::atomic::AtomicU64,
    clock: Clock,
    visible: Arc<std::sync::atomic::AtomicBool>,
    /// Each new link's silence timeout (`LinkCfg::silence_ms`).
    silence_ms: std::sync::atomic::AtomicU64,
}

pub struct Core {
    inner: Arc<Inner>,
    rt: Mutex<Option<Runtime>>,
}

impl Core {
    /// A core over real sockets, keeping its files in `dir`.
    pub fn new(dir: PathBuf, sb: Arc<dyn SecretBox>, device_name: String) -> Result<Core, String> {
        Self::with_connector(dir, sb, device_name, Arc::new(WsConnector))
    }

    pub fn with_connector(
        dir: PathBuf,
        sb: Arc<dyn SecretBox>,
        device_name: String,
        connector: Arc<dyn Connector>,
    ) -> Result<Core, String> {
        let identity = Arc::new(Identity::load_or_create(&dir, sb.as_ref())?);
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("wado-core")
            .enable_all()
            .build()
            .map_err(|e| format!("runtime: {e}"))?;
        let start = Instant::now();
        let inner = Inner {
            identity,
            profiles: Arc::new(Mutex::new(Profiles::load(&dir))),
            pins: Arc::new(Mutex::new(Pins::load(&dir))),
            name: device_name,
            connector,
            events: Arc::new(EventQueue::default()),
            ops: Mutex::new(Ops::default()),
            links: Mutex::new(HashMap::new()),
            rooms: Mutex::new(HashMap::new()),
            next_room: std::sync::atomic::AtomicU64::new(1),
            clock: Arc::new(move || start.elapsed().as_millis() as u64),
            visible: Arc::new(std::sync::atomic::AtomicBool::new(true)),
            silence_ms: std::sync::atomic::AtomicU64::new(crate::link::state::SILENCE_MS),
        };
        Ok(Core {
            inner: Arc::new(inner),
            rt: Mutex::new(Some(rt)),
        })
    }

    /// This device's key on every computer's trust list.
    pub fn client_key(&self) -> String {
        self.inner.identity.client_key.clone()
    }

    pub fn computers(&self) -> Vec<ComputerView> {
        self.inner
            .profiles
            .lock()
            .unwrap()
            .list()
            .iter()
            .map(ComputerView::of)
            .collect()
    }

    fn emit_computers(&self) {
        let list = self.computers();
        self.inner.events.push(CoreEvent::Computers { list });
    }

    fn spawn<F: std::future::Future<Output = ()> + Send + 'static>(
        &self,
        f: F,
    ) -> Option<JoinHandle<()>> {
        self.rt.lock().unwrap().as_ref().map(|rt| rt.spawn(f))
    }

    /// The UI is on screen (polls the host state and sessions) or not (links stay up, quiet — a
    /// background holder such as push without a distributor). On by default.
    pub fn set_visible(&self, on: bool) {
        self.inner
            .visible
            .store(on, std::sync::atomic::Ordering::Relaxed);
    }

    /// How long a link may hear nothing (the relay pings every 15 s) before it redials — for links
    /// opened from now on. A long-lived background holder keeps the default or lowers it for a
    /// network that drops idle sockets sooner.
    pub fn set_keepalive(&self, silence_ms: u64) {
        self.inner
            .silence_ms
            .store(silence_ms.max(20_000), std::sync::atomic::Ordering::Relaxed);
    }

    /// Hold a control room for exactly `keys` (the computers on screen), and drop the rest.
    pub fn watch(&self, keys: Vec<String>) {
        let mut links = self.inner.links.lock().unwrap();
        links.retain(|k, w| {
            let keep = keys.contains(k);
            if !keep {
                let _ = w.cmds.send(LinkCmd::Stop);
            }
            keep
        });
        for key in keys {
            if links.contains_key(&key) {
                continue;
            }
            let Some(p) = self.inner.profiles.lock().unwrap().get(&key).cloned() else {
                continue;
            };
            let pin = self.inner.pins.lock().unwrap().get(&p.remote_id);
            let link = Link::new(
                LinkCfg {
                    silence_ms: self
                        .inner
                        .silence_ms
                        .load(std::sync::atomic::Ordering::Relaxed),
                    ..LinkCfg::new(&key, &p.relay_url, &p.remote_id, &self.inner.name)
                },
                self.inner.identity.clone(),
                pin,
            );
            let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
            let (out_tx, out_rx) = mpsc::unbounded_channel();
            let pump = Pump {
                key: key.clone(),
                remote_id: p.remote_id.clone(),
                cmds: cmd_tx.clone(),
                events: self.inner.events.clone(),
                profiles: self.inner.profiles.clone(),
                pins: self.inner.pins.clone(),
                clock: self.inner.clock.clone(),
                visible: self.inner.visible.clone(),
            };
            let tasks = [
                self.spawn(driver::run(
                    link,
                    self.inner.connector.clone(),
                    out_tx,
                    cmd_rx,
                    self.inner.clock.clone(),
                )),
                self.spawn(pump.run(out_rx)),
            ]
            .into_iter()
            .flatten()
            .collect();
            links.insert(
                key,
                Watched {
                    cmds: cmd_tx,
                    tasks,
                },
            );
        }
    }

    /// Open a room on the saved computer `key`: the seat, a control room, or a view room on
    /// `instance`. Its state, messages and relay frames come as `Room*{id, …}` events until
    /// `close_room`. `None` if this device does not know the computer.
    pub fn open_room(&self, key: String, room: Room) -> Option<String> {
        let p = self.inner.profiles.lock().unwrap().get(&key).cloned()?;
        let pin = self.inner.pins.lock().unwrap().get(&p.remote_id);
        let n = self
            .inner
            .next_room
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let id = format!("r{n}");
        let link = Link::new(
            LinkCfg {
                silence_ms: self
                    .inner
                    .silence_ms
                    .load(std::sync::atomic::Ordering::Relaxed),
                room,
                ..LinkCfg::new(&key, &p.relay_url, &p.remote_id, &self.inner.name)
            },
            self.inner.identity.clone(),
            pin,
        );
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
        let (out_tx, out_rx) = mpsc::unbounded_channel();
        let pump = crate::rooms::RoomPump {
            id: id.clone(),
            remote_id: p.remote_id.clone(),
            events: self.inner.events.clone(),
            pins: self.inner.pins.clone(),
            clock: self.inner.clock.clone(),
        };
        let tasks = [
            self.spawn(driver::run(
                link,
                self.inner.connector.clone(),
                out_tx,
                cmd_rx,
                self.inner.clock.clone(),
            )),
            self.spawn(pump.run(out_rx)),
        ]
        .into_iter()
        .flatten()
        .collect();
        self.inner.rooms.lock().unwrap().insert(
            id.clone(),
            Watched {
                cmds: cmd_tx,
                tasks,
            },
        );
        Some(id)
    }

    /// A room's UI read the computer's host state: refresh the saved name (until the person names
    /// it), OS and desktop, as a watched link's pump does, so cards and logos follow the computer.
    pub fn note_host(&self, key: &str, state_json: &str) {
        let Ok(state) = serde_json::from_str::<wado_protocol::HostState>(state_json) else {
            return;
        };
        let v = crate::feeds::host::view(&state);
        let changed = {
            let mut ps = self.inner.profiles.lock().unwrap();
            let Some(mut p) = ps.get(key).cloned() else {
                return;
            };
            let before = p.clone();
            if !v.name.is_empty()
                && p.name == wado_protocol::relay_wire::display_remote_id(&p.remote_id)
            {
                p.name = v.name.clone();
            }
            p.os_id = v.os_id.clone();
            p.os_like = v.os_like.clone();
            p.desktop = v.desktop.clone();
            let changed = p != before;
            if changed {
                ps.upsert(p);
                let _ = ps.save();
            }
            changed
        };
        if changed {
            self.emit_computers();
        }
    }

    /// Send plaintext JSON to a room's computer, sealed as it leaves. Urgent jumps the bulk
    /// queue; `resend` sends it again after a reconnect. False if the room is closed.
    pub fn send_room(&self, id: &str, json: String, urgent: bool, resend: bool) -> bool {
        let rooms = self.inner.rooms.lock().unwrap();
        let Some(w) = rooms.get(id) else { return false };
        w.cmds
            .send(LinkCmd::Send {
                text: json,
                class: if urgent { Class::Urgent } else { Class::Bulk },
                resend,
            })
            .is_ok()
    }

    /// Leave a room (4001) and forget it.
    pub fn close_room(&self, id: &str) {
        if let Some(w) = self.inner.rooms.lock().unwrap().remove(id) {
            let _ = w.cmds.send(LinkCmd::Stop);
        }
    }

    fn start_pair(&self, link: PairLink) -> u64 {
        let op = self.inner.ops.lock().unwrap().next_id();
        let inner = self.inner.clone();
        let ctx = PairCtx {
            identity: inner.identity.clone(),
            profiles: inner.profiles.clone(),
            pins: inner.pins.clone(),
            name: inner.name.clone(),
        };
        self.spawn(async move {
            let events = inner.events.clone();
            let last = flow::run(link, &ctx, inner.connector.as_ref(), |step| {
                events.push(CoreEvent::Pair { op, step })
            })
            .await;
            if matches!(last, PairStep::Done { .. }) {
                let list = inner
                    .profiles
                    .lock()
                    .unwrap()
                    .list()
                    .iter()
                    .map(ComputerView::of)
                    .collect();
                inner.events.push(CoreEvent::Computers { list });
            }
        });
        op
    }

    /// Pair with a scanned or pasted link. Progress comes as `Pair{op, …}` events.
    pub fn pair(&self, link: String) -> u64 {
        match pairlink::parse(&link) {
            Ok(l) => self.start_pair(l),
            Err(e) => self.fail_pair(e.to_string()),
        }
    }

    /// Pair with a code typed from the computer's screen.
    pub fn pair_typed(&self, relay: String, rid: String, code: String) -> u64 {
        let Some(code) = pairlink::typed(&code) else {
            return self.fail_pair(
                "That code isn't eight letters and digits — check it on the computer.".into(),
            );
        };
        let remote_id: String = rid.chars().filter(char::is_ascii_digit).collect();
        let relay = relay.trim().trim_end_matches('/').to_string();
        if remote_id.is_empty() || !(relay.starts_with("https://") || relay.starts_with("http://"))
        {
            return self
                .fail_pair("Enter the relay address and Remote ID the computer shows.".into());
        }
        self.start_pair(PairLink {
            relay_url: relay,
            remote_id,
            code: PairCode::Typed(code),
            hk: None,
        })
    }

    fn fail_pair(&self, why: String) -> u64 {
        let op = self.inner.ops.lock().unwrap().next_id();
        self.inner.events.push(CoreEvent::Pair {
            op,
            step: PairStep::Failed { why },
        });
        op
    }

    /// Forget a computer on this device: its link, profile and pin.
    pub fn forget(&self, key: String) -> u64 {
        let op = self.inner.ops.lock().unwrap().next_id();
        if let Some(w) = self.inner.links.lock().unwrap().remove(&key) {
            let _ = w.cmds.send(LinkCmd::Stop);
        }
        let rid = {
            let mut ps = self.inner.profiles.lock().unwrap();
            let rid = ps.get(&key).map(|p| p.remote_id.clone());
            ps.forget(&key);
            let saved = ps.save();
            (rid, saved)
        };
        let outcome = match rid {
            (Some(rid), Ok(())) => {
                let _ = self.inner.pins.lock().unwrap().forget(&rid);
                Outcome::Done
            }
            (None, _) => Outcome::Failed("This phone doesn't know that computer.".into()),
            (_, Err(e)) => Outcome::Failed(format!("Couldn't save: {e}")),
        };
        self.emit_computers();
        self.inner.events.push(CoreEvent::Answer { op, outcome });
        op
    }

    /// The next event, waiting for one.
    pub async fn next_event(&self) -> CoreEvent {
        self.inner.events.pop().await
    }

    /// Leave every room (4001) and stop the runtime. Waits briefly for the leaves to go out.
    pub fn shutdown(&self) {
        let mut watched: Vec<Watched> = self
            .inner
            .links
            .lock()
            .unwrap()
            .drain()
            .map(|(_, w)| w)
            .collect();
        watched.extend(self.inner.rooms.lock().unwrap().drain().map(|(_, w)| w));
        for w in &watched {
            let _ = w.cmds.send(LinkCmd::Stop);
        }
        let Some(rt) = self.rt.lock().unwrap().take() else {
            return;
        };
        rt.block_on(async {
            for w in watched {
                for t in w.tasks {
                    let _ = tokio::time::timeout(Duration::from_secs(1), t).await;
                }
            }
        });
        rt.shutdown_timeout(Duration::from_millis(500));
    }
}

impl Drop for Core {
    fn drop(&mut self) {
        if let Some(rt) = self.rt.get_mut().ok().and_then(Option::take) {
            rt.shutdown_background();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profiles::Profile;
    use crate::relay::transport::{MemTransport, Transport};
    use crate::secret::XorBox;
    use async_trait::async_trait;

    /// Each dial makes a socket pair; the far end goes to the test.
    struct MemConnector(Mutex<mpsc::UnboundedSender<(String, MemTransport)>>);

    #[async_trait]
    impl Connector for MemConnector {
        async fn connect(&self, url: &str) -> std::io::Result<Box<dyn Transport>> {
            let (a, b) = MemTransport::pair();
            let _ = self.0.lock().unwrap().send((url.to_string(), b));
            Ok(Box::new(a))
        }
    }

    fn core() -> (Core, mpsc::UnboundedReceiver<(String, MemTransport)>) {
        let dir = crate::store::temp_dir("core");
        let mut ps = Profiles::load(&dir);
        ps.upsert(Profile::new("https://relay.example", "111"));
        ps.upsert(Profile::new("https://relay.example", "222"));
        ps.save().unwrap();
        let (tx, rx) = mpsc::unbounded_channel();
        let c = Core::with_connector(
            dir,
            Arc::new(XorBox(7)),
            "Phone".into(),
            Arc::new(MemConnector(Mutex::new(tx))),
        )
        .unwrap();
        (c, rx)
    }

    fn block<T>(f: impl std::future::Future<Output = T>) -> T {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(f)
    }

    async fn dialed(
        rx: &mut mpsc::UnboundedReceiver<(String, MemTransport)>,
    ) -> (String, MemTransport) {
        tokio::time::timeout(Duration::from_secs(5), rx.recv())
            .await
            .expect("a dial")
            .unwrap()
    }

    async fn closed_with(t: &mut MemTransport) -> Option<u16> {
        let _ = tokio::time::timeout(Duration::from_secs(5), async {
            while t.recv().await.is_some() {}
        })
        .await;
        *t.closed_by_peer.lock().unwrap()
    }

    #[test]
    fn watch_opens_and_drops_links() {
        let (c, mut rx) = core();
        assert_eq!(c.computers().len(), 2);
        c.watch(vec!["111".into(), "unknown".into()]);
        block(async {
            let (url, mut far) = dialed(&mut rx).await;
            assert!(
                url.contains("/join/111?") && url.contains("control=1&strict=1"),
                "{url}"
            );
            // Watching the same set again dials nothing new.
            c.watch(vec!["111".into()]);
            // Dropped from the watch: it leaves.
            c.watch(vec!["222".into()]);
            assert_eq!(closed_with(&mut far).await, Some(4001));
            let (url, _far2) = dialed(&mut rx).await;
            assert!(url.contains("/join/222?"), "{url}");
            assert!(rx.try_recv().is_err(), "nothing else dialled");
        });
        c.shutdown();
    }

    #[test]
    fn shutdown_closes_with_leave_code() {
        let (c, mut rx) = core();
        c.watch(vec!["111".into()]);
        let mut far = block(async { dialed(&mut rx).await.1 });
        c.shutdown();
        block(async { assert_eq!(closed_with(&mut far).await, Some(4001)) });
    }

    #[test]
    fn link_and_forget_report_as_events() {
        let (c, mut rx) = core();
        c.watch(vec!["111".into()]);
        block(async {
            let (_, _far) = dialed(&mut rx).await;
            let ev = tokio::time::timeout(Duration::from_secs(5), c.next_event())
                .await
                .unwrap();
            assert!(
                matches!(ev, CoreEvent::Link { ref key, .. } if key == "111"),
                "{ev:?}"
            );
            let op = c.forget("111".into());
            let mut got = Vec::new();
            while let Ok(ev) =
                tokio::time::timeout(Duration::from_millis(500), c.next_event()).await
            {
                got.push(ev);
            }
            assert!(got.contains(&CoreEvent::Answer {
                op,
                outcome: Outcome::Done
            }));
            assert!(got
                .iter()
                .any(|e| matches!(e, CoreEvent::Computers { list } if list.len() == 1)));
        });
        // A bad link fails at once, plainly.
        let op = c.pair("not a link".into());
        let ev = block(async { c.next_event().await });
        assert_eq!(
            ev,
            CoreEvent::Pair {
                op,
                step: PairStep::Failed {
                    why: "that is not a wado pairing link".into()
                }
            }
        );
        c.shutdown();
    }
}
