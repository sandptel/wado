//! Fake control-plane capabilities. Each records its calls in a shared [`Log`] as readable lines
//! (`"audio.set_volume 3 0.5"`), and each stateful one publishes whatever the test says the next
//! fetch will read ([`Stateful::will_read`]), counting fetches so a test can measure polling.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::{mpsc, watch};
use wado_protocol::ClipEntry;
use wado_protocol::host::{Bluetooth as BtState, HostAudio, MediaOp, Player, SysInfo, Wifi};

use crate::audio::{AudioMixer, SinkHandle, ViewerSink};
use crate::clip::{Clip, ClipHistory, Clipboard};
use crate::desktop::{App, Launcher, Screenshot, WindowManager, WindowOp};
use crate::display::Brightness;
use crate::files::{Item, ThumbSource, Thumbnailer, Trash};
use crate::media::MediaPlayers;
use crate::network::{Bluetooth, WifiRadio};
use crate::power::{PowerInhibit, PowerOp, SystemPower};
use crate::pty::{PtyRequest, PtySession, PtySpawner};
use crate::state::Published;
use crate::sysinfo::SysInfoSource;
use crate::{Fut, Res};

/// Every call the fakes took, as lines, in order.
#[derive(Clone, Default)]
pub struct Log(Arc<Mutex<Vec<String>>>);

impl Log {
    pub fn calls(&self) -> Vec<String> {
        self.0.lock().unwrap().clone()
    }

    pub fn push(&self, line: impl Into<String>) {
        self.0.lock().unwrap().push(line.into());
    }

    /// Forget what was recorded so far.
    pub fn clear(&self) {
        self.0.lock().unwrap().clear();
    }
}

/// A fake's published state, and what its next fetch will read.
pub struct Stateful<S> {
    pub published: Published<S>,
    next: Mutex<S>,
}

impl<S: Clone + PartialEq + Send + Sync> Stateful<S> {
    pub fn new(init: S) -> Self {
        Stateful {
            published: Published::new(init.clone()),
            next: Mutex::new(init),
        }
    }

    /// What the next fetch reads — the "real" state behind the fake.
    pub fn will_read(&self, s: S) {
        *self.next.lock().unwrap() = s;
    }

    /// Fetches run so far.
    pub fn fetches(&self) -> u64 {
        self.published.fetches()
    }

    async fn fresh(&self, within: Duration) {
        let next = self.next.lock().unwrap().clone();
        self.published.fresh(within, || async { next }).await;
    }
}

/// `Ok`, after writing `line` to `log`.
fn ok(log: &Log, line: String) -> Res<()> {
    log.push(line);
    Ok(())
}

pub struct FakeAudio {
    pub state: Stateful<HostAudio>,
    pub log: Log,
}

impl AudioMixer for FakeAudio {
    fn state(&self) -> watch::Receiver<HostAudio> {
        self.state.published.subscribe()
    }
    fn fresh(&self, within: Duration) -> Fut<'_, ()> {
        Box::pin(self.state.fresh(within))
    }
    fn set_volume(&self, id: u32, volume: f32) -> Fut<'_, Res<()>> {
        Box::pin(async move { ok(&self.log, format!("audio.set_volume {id} {volume}")) })
    }
    fn set_mute(&self, id: u32, muted: bool) -> Fut<'_, Res<()>> {
        Box::pin(async move { ok(&self.log, format!("audio.set_mute {id} {muted}")) })
    }
    fn set_default(&self, name: String) -> Fut<'_, Res<()>> {
        Box::pin(async move { ok(&self.log, format!("audio.set_default {name}")) })
    }
    fn move_stream(&self, id: u32, sink: String) -> Fut<'_, Res<()>> {
        Box::pin(async move { ok(&self.log, format!("audio.move_stream {id} {sink}")) })
    }
}

pub struct FakeViewerSink {
    pub log: Log,
}

struct FakeSinkHandle {
    log: Log,
    description: String,
}

impl SinkHandle for FakeSinkHandle {}

impl Drop for FakeSinkHandle {
    fn drop(&mut self) {
        self.log.push(format!("sink.close {}", self.description));
    }
}

impl ViewerSink for FakeViewerSink {
    fn name(&self) -> String {
        "wado-fake".into()
    }
    fn open(&self, description: String) -> Res<Box<dyn SinkHandle>> {
        self.log.push(format!("sink.open {description}"));
        Ok(Box::new(FakeSinkHandle {
            log: self.log.clone(),
            description,
        }))
    }
}

pub struct FakeMedia {
    pub state: Stateful<Vec<Player>>,
    pub log: Log,
}

impl MediaPlayers for FakeMedia {
    fn state(&self) -> watch::Receiver<Vec<Player>> {
        self.state.published.subscribe()
    }
    fn fresh(&self, within: Duration) -> Fut<'_, ()> {
        Box::pin(self.state.fresh(within))
    }
    fn act(&self, bus: String, op: MediaOp) -> Fut<'_, Res<()>> {
        Box::pin(async move { ok(&self.log, format!("media.act {bus} {op:?}")) })
    }
}

pub struct FakeBrightness {
    pub state: Stateful<Option<f32>>,
    pub log: Log,
}

impl Brightness for FakeBrightness {
    fn state(&self) -> watch::Receiver<Option<f32>> {
        self.state.published.subscribe()
    }
    fn fresh(&self, within: Duration) -> Fut<'_, ()> {
        Box::pin(self.state.fresh(within))
    }
    fn set(&self, level: f32) -> Fut<'_, Res<()>> {
        Box::pin(async move { ok(&self.log, format!("brightness.set {level}")) })
    }
    fn step(&self, up: bool) -> Fut<'_, Res<()>> {
        Box::pin(async move { ok(&self.log, format!("brightness.step {up}")) })
    }
}

pub struct FakeWifi {
    pub state: Stateful<Option<Wifi>>,
    pub log: Log,
}

impl WifiRadio for FakeWifi {
    fn state(&self) -> watch::Receiver<Option<Wifi>> {
        self.state.published.subscribe()
    }
    fn fresh(&self, within: Duration) -> Fut<'_, ()> {
        Box::pin(self.state.fresh(within))
    }
    fn radio(&self, on: bool) -> Fut<'_, Res<()>> {
        Box::pin(async move { ok(&self.log, format!("wifi.radio {on}")) })
    }
    fn join(&self, ssid: String, password: Option<String>) -> Fut<'_, Res<()>> {
        Box::pin(async move {
            let pw = if password.is_some() {
                " (password)"
            } else {
                ""
            };
            ok(&self.log, format!("wifi.join {ssid}{pw}"))
        })
    }
    fn rejoin(&self, name: String) -> Fut<'_, Res<()>> {
        Box::pin(async move { ok(&self.log, format!("wifi.rejoin {name}")) })
    }
}

pub struct FakeBluetooth {
    pub state: Stateful<Option<BtState>>,
    pub log: Log,
}

impl Bluetooth for FakeBluetooth {
    fn state(&self) -> watch::Receiver<Option<BtState>> {
        self.state.published.subscribe()
    }
    fn fresh(&self, within: Duration) -> Fut<'_, ()> {
        Box::pin(self.state.fresh(within))
    }
    fn power(&self, on: bool) -> Fut<'_, Res<()>> {
        Box::pin(async move { ok(&self.log, format!("bt.power {on}")) })
    }
    fn connect(&self, addr: String, connect: bool) -> Fut<'_, Res<()>> {
        Box::pin(async move { ok(&self.log, format!("bt.connect {addr} {connect}")) })
    }
}

pub struct FakeInhibit {
    pub state: Published<bool>,
    pub log: Log,
}

impl PowerInhibit for FakeInhibit {
    fn state(&self) -> watch::Receiver<bool> {
        self.state.subscribe()
    }
    fn set(&self, on: bool) -> Res<()> {
        self.log.push(format!("awake.set {on}"));
        self.state.publish(on);
        Ok(())
    }
}

pub struct FakePower {
    pub log: Log,
    pub can: Vec<PowerOp>,
}

impl SystemPower for FakePower {
    fn can(&self, op: PowerOp) -> Res<()> {
        self.can
            .contains(&op)
            .then_some(())
            .ok_or_else(|| format!("{op:?} is not possible here"))
    }
    fn run(&self, op: PowerOp) -> Fut<'_, Res<()>> {
        Box::pin(async move { ok(&self.log, format!("power.run {op:?}")) })
    }
}

pub struct FakeSysInfo;

impl SysInfoSource for FakeSysInfo {
    fn now(&self) -> SysInfo {
        SysInfo {
            hostname: "fake".into(),
            ..SysInfo::default()
        }
    }
}

pub struct FakeClipboard {
    pub held: Mutex<Option<Clip>>,
    pub changes: watch::Sender<u64>,
    pub log: Log,
}

impl Clipboard for FakeClipboard {
    fn get(&self) -> Fut<'_, Res<Clip>> {
        Box::pin(async move {
            self.held
                .lock()
                .unwrap()
                .clone()
                .ok_or_else(|| "nothing we can pass on".to_string())
        })
    }
    fn set(&self, clip: Clip) -> Fut<'_, Res<()>> {
        Box::pin(async move {
            self.log.push(format!("clip.set {}", clip.mime));
            *self.held.lock().unwrap() = Some(clip);
            self.changes.send_modify(|n| *n += 1);
            Ok(())
        })
    }
    fn changes(&self) -> watch::Receiver<u64> {
        self.changes.subscribe()
    }
}

pub struct FakeHistory {
    pub entries: Mutex<Vec<ClipEntry>>,
    pub log: Log,
}

impl ClipHistory for FakeHistory {
    fn list(&self) -> Fut<'_, Res<Vec<ClipEntry>>> {
        Box::pin(async move { Ok(self.entries.lock().unwrap().clone()) })
    }
    fn get(&self, id: String) -> Fut<'_, Res<Clip>> {
        Box::pin(async move {
            let e = self.entries.lock().unwrap();
            let e = e
                .iter()
                .find(|e| e.id == id)
                .ok_or("not a clipboard entry")?;
            Ok(Clip {
                mime: "text/plain;charset=utf-8".into(),
                data: e.preview.as_bytes().into(),
                ..Default::default()
            })
        })
    }
    fn delete(&self, id: String) -> Fut<'_, Res<()>> {
        Box::pin(async move {
            self.entries.lock().unwrap().retain(|e| e.id != id);
            ok(&self.log, format!("history.delete {id}"))
        })
    }
    fn pin(&self, id: String, on: bool) -> Fut<'_, Res<()>> {
        Box::pin(async move {
            for e in self
                .entries
                .lock()
                .unwrap()
                .iter_mut()
                .filter(|e| e.id == id)
            {
                e.pinned = on;
            }
            ok(&self.log, format!("history.pin {id} {on}"))
        })
    }
}

pub struct FakeWindows {
    pub log: Log,
}

impl WindowManager for FakeWindows {
    fn run(&self, op: WindowOp) -> Fut<'_, Res<()>> {
        Box::pin(async move { ok(&self.log, format!("window.run {op:?}")) })
    }
}

pub struct FakeLauncher {
    pub log: Log,
    pub can: Vec<App>,
}

impl Launcher for FakeLauncher {
    fn can(&self, app: App) -> Res<()> {
        self.can
            .contains(&app)
            .then_some(())
            .ok_or_else(|| format!("{app:?} cannot be opened here"))
    }
    fn open(&self, app: App) -> Res<()> {
        ok(&self.log, format!("launch.open {app:?}"))
    }
}

pub struct FakeScreenshot {
    pub log: Log,
    pub region: bool,
}

impl Screenshot for FakeScreenshot {
    fn can_region(&self) -> Res<()> {
        self.region
            .then_some(())
            .ok_or_else(|| "no area can be chosen here".to_string())
    }
    fn shoot(&self, region: bool, to: PathBuf) -> Fut<'_, Res<()>> {
        Box::pin(async move { ok(&self.log, format!("shot {region} {}", to.display())) })
    }
}

pub struct FakeTrash {
    pub log: Log,
}

impl Trash for FakeTrash {
    fn place(&self) -> PathBuf {
        PathBuf::from("/fake/Trash/files")
    }
    fn trash(&self, item: Item<'_>) -> Res<()> {
        ok(
            &self.log,
            format!("trash {}", item.dir_path.join(item.name).display()),
        )
    }
    fn origin(&self, item: Item<'_>) -> Res<PathBuf> {
        Err(format!("no record of where {} came from", item.name))
    }
    fn forget(&self, item: Item<'_>) {
        self.log.push(format!("trash.forget {}", item.name));
    }
}

pub struct FakeThumbs {
    pub log: Log,
}

impl Thumbnailer for FakeThumbs {
    fn thumb(&self, src: ThumbSource) -> Fut<'_, Res<Vec<u8>>> {
        Box::pin(async move {
            self.log.push(format!("thumb {}", src.real.display()));
            Ok(vec![0x89, b'P', b'N', b'G'])
        })
    }
}

pub struct FakePty {
    pub log: Log,
}

struct FakeSession {
    log: Log,
    out: mpsc::Sender<String>,
}

impl PtySession for FakeSession {
    fn write(&mut self, data: &str) -> Res<()> {
        self.log.push(format!("pty.write {data:?}"));
        // An echoing terminal.
        let _ = self.out.try_send(data.to_string());
        Ok(())
    }
    fn resize(&mut self, cols: u16, rows: u16) {
        self.log.push(format!("pty.resize {cols}x{rows}"));
    }
    fn exited(&mut self) -> Option<Option<i32>> {
        None
    }
}

impl PtySpawner for FakePty {
    fn spawn(&self, req: PtyRequest, out: mpsc::Sender<String>) -> Res<Box<dyn PtySession>> {
        self.log.push(format!(
            "pty.spawn {:?} {}x{}",
            req.program, req.cols, req.rows
        ));
        Ok(Box::new(FakeSession {
            log: self.log.clone(),
            out,
        }))
    }
}
