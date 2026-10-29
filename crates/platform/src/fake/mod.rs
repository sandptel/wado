//! A fake for every capability (`fake` feature), and the contract every provider must pass.
//!
//! [`Fakes::new`] makes one of each, recording into shared logs; [`Fakes::provider`] offers them
//! all, so a test builds a whole [`crate::Platform`] of fakes — and can still leave capabilities
//! out with [`crate::PlatformBuilder::without`] to test honest absence.
//!
//! - [`input`] — fake devices and typist, recording into an [`input::InputLog`].
//! - [`host`] — fake control-plane capabilities, recording into a [`host::Log`].
//! - [`contract`] — what any provider of a capability must do, checked against a real one or a
//!   fake.

pub mod contract;
pub mod host;
pub mod input;

use std::sync::{Arc, Mutex};

use tokio::sync::watch;

use crate::audio::{AudioMixer, ViewerSink};
use crate::cap::ProviderId;
use crate::clip::{ClipHistory, Clipboard};
use crate::desktop::{App, Launcher, Screenshot, WindowManager};
use crate::display::Brightness;
use crate::files::{Thumbnailer, Trash};
use crate::input::{GamepadInjector, KeyInjector, Opener, TextInjector, TouchInjector};
use crate::media::MediaPlayers;
use crate::network::{Bluetooth, WifiRadio};
use crate::power::{PowerInhibit, PowerOp, SystemPower};
use crate::pty::PtySpawner;
use crate::registry::{Offer, Provider};
use crate::state::Published;
use crate::sysinfo::SysInfoSource;
use crate::{Platform, PlatformBuilder};

use host::*;
use input::InputLog;

/// The fake provider's id.
pub const FAKE: ProviderId = ProviderId("fake");

/// One fake of every capability, and the logs they record into.
#[derive(Clone)]
pub struct Fakes {
    pub input: InputLog,
    pub log: Log,
    pub audio: Arc<FakeAudio>,
    pub viewer_sink: Arc<FakeViewerSink>,
    pub media: Arc<FakeMedia>,
    pub brightness: Arc<FakeBrightness>,
    pub wifi: Arc<FakeWifi>,
    pub bluetooth: Arc<FakeBluetooth>,
    pub awake: Arc<FakeInhibit>,
    pub power: Arc<FakePower>,
    pub clipboard: Arc<FakeClipboard>,
    pub history: Arc<FakeHistory>,
    pub windows: Arc<FakeWindows>,
    pub launcher: Arc<FakeLauncher>,
    pub screenshot: Arc<FakeScreenshot>,
    pub trash: Arc<FakeTrash>,
    pub thumbs: Arc<FakeThumbs>,
    pub pty: Arc<FakePty>,
}

impl Default for Fakes {
    fn default() -> Self {
        Self::new()
    }
}

impl Fakes {
    pub fn new() -> Fakes {
        let log = Log::default();
        Fakes {
            input: InputLog::default(),
            audio: Arc::new(FakeAudio {
                state: Stateful::new(Default::default()),
                log: log.clone(),
            }),
            viewer_sink: Arc::new(FakeViewerSink { log: log.clone() }),
            media: Arc::new(FakeMedia {
                state: Stateful::new(Vec::new()),
                log: log.clone(),
            }),
            brightness: Arc::new(FakeBrightness {
                state: Stateful::new(None),
                log: log.clone(),
            }),
            wifi: Arc::new(FakeWifi {
                state: Stateful::new(None),
                log: log.clone(),
            }),
            bluetooth: Arc::new(FakeBluetooth {
                state: Stateful::new(None),
                log: log.clone(),
            }),
            awake: Arc::new(FakeInhibit {
                state: Published::new(false),
                log: log.clone(),
            }),
            power: Arc::new(FakePower {
                log: log.clone(),
                can: vec![PowerOp::Lock, PowerOp::Suspend],
            }),
            clipboard: Arc::new(FakeClipboard {
                held: Mutex::new(None),
                changes: watch::channel(0).0,
                log: log.clone(),
            }),
            history: Arc::new(FakeHistory {
                entries: Mutex::new(Vec::new()),
                log: log.clone(),
            }),
            windows: Arc::new(FakeWindows { log: log.clone() }),
            launcher: Arc::new(FakeLauncher {
                log: log.clone(),
                can: vec![App::Terminal, App::Browser, App::Files],
            }),
            screenshot: Arc::new(FakeScreenshot {
                log: log.clone(),
                region: true,
            }),
            trash: Arc::new(FakeTrash { log: log.clone() }),
            thumbs: Arc::new(FakeThumbs { log: log.clone() }),
            pty: Arc::new(FakePty { log: log.clone() }),
            log,
        }
    }

    /// A provider offering every fake.
    pub fn provider(&self) -> FakeProvider {
        FakeProvider(self.clone())
    }

    /// A platform of nothing but these fakes.
    pub fn platform(&self) -> Platform {
        PlatformBuilder::empty().with(self.provider()).probe()
    }
}

/// Offers every capability, from a [`Fakes`].
pub struct FakeProvider(pub Fakes);

fn on<T: ?Sized>(imp: Arc<T>) -> Offer<T> {
    Some(Ok(imp))
}

impl Provider for FakeProvider {
    fn id(&self) -> ProviderId {
        FAKE
    }
    fn touch(&self) -> Offer<dyn Opener<dyn TouchInjector>> {
        on(self.0.input.touch())
    }
    fn keys(&self) -> Offer<dyn Opener<dyn KeyInjector>> {
        on(self.0.input.keys())
    }
    fn gamepad(&self) -> Offer<dyn Opener<dyn GamepadInjector>> {
        on(self.0.input.gamepad())
    }
    fn text(&self) -> Offer<dyn TextInjector> {
        on(self.0.input.text())
    }
    fn audio(&self) -> Offer<dyn AudioMixer> {
        on(self.0.audio.clone())
    }
    fn viewer_sink(&self) -> Offer<dyn ViewerSink> {
        on(self.0.viewer_sink.clone())
    }
    fn media(&self) -> Offer<dyn MediaPlayers> {
        on(self.0.media.clone())
    }
    fn brightness(&self) -> Offer<dyn Brightness> {
        on(self.0.brightness.clone())
    }
    fn wifi(&self) -> Offer<dyn WifiRadio> {
        on(self.0.wifi.clone())
    }
    fn bluetooth(&self) -> Offer<dyn Bluetooth> {
        on(self.0.bluetooth.clone())
    }
    fn power_inhibit(&self) -> Offer<dyn PowerInhibit> {
        on(self.0.awake.clone())
    }
    fn system_power(&self) -> Offer<dyn SystemPower> {
        on(self.0.power.clone())
    }
    fn clipboard(&self) -> Offer<dyn Clipboard> {
        on(self.0.clipboard.clone())
    }
    fn clip_history(&self) -> Offer<dyn ClipHistory> {
        on(self.0.history.clone())
    }
    fn window_manager(&self) -> Offer<dyn WindowManager> {
        on(self.0.windows.clone())
    }
    fn launcher(&self) -> Offer<dyn Launcher> {
        on(self.0.launcher.clone())
    }
    fn screenshot(&self) -> Offer<dyn Screenshot> {
        on(self.0.screenshot.clone())
    }
    fn sys_info(&self) -> Offer<dyn SysInfoSource> {
        on(Arc::new(FakeSysInfo))
    }
    fn trash(&self) -> Offer<dyn Trash> {
        on(self.0.trash.clone())
    }
    fn thumbnailer(&self) -> Offer<dyn Thumbnailer> {
        on(self.0.thumbs.clone())
    }
    fn pty(&self) -> Offer<dyn PtySpawner> {
        on(self.0.pty.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Capability;
    use crate::registry::{LEFT_OUT, NOBODY};

    /// A provider that tries Wi-Fi and fails, and offers nothing else.
    struct NoWifi;
    impl Provider for NoWifi {
        fn id(&self) -> ProviderId {
            ProviderId("nowifi")
        }
        fn wifi(&self) -> Offer<dyn WifiRadio> {
            Some(Err("NetworkManager is not running".into()))
        }
    }

    #[test]
    fn every_capability_is_on_from_the_fakes() {
        let p = Fakes::new().platform();
        for cap in Capability::ALL {
            assert_eq!(p.by(cap), Some(FAKE), "{cap:?}");
        }
        assert!(p.caps().off.is_empty());
    }

    #[test]
    fn absence_is_honest() {
        let p = PlatformBuilder::empty()
            .with(NoWifi)
            .with(Fakes::new().provider())
            .without(Capability::Audio)
            .probe();
        // Tried and failed here, then the fakes had it.
        assert_eq!(p.by(Capability::Wifi), Some(FAKE));
        let p = PlatformBuilder::empty().with(NoWifi).probe();
        let a = p.wifi().err().unwrap();
        assert_eq!(a.why, "NetworkManager is not running");
        assert_eq!(a.tried, vec![(ProviderId("nowifi"), a.why.clone())]);
        assert_eq!(p.audio().err().unwrap().why, NOBODY);
        let caps = p.caps();
        assert!(caps.on.is_empty());
        assert!(caps.off.iter().any(|o| o.cap == "wifi" && o.why == a.why));
        let left = PlatformBuilder::empty()
            .with(Fakes::new().provider())
            .without(Capability::Audio)
            .probe();
        assert_eq!(left.absent(Capability::Audio).unwrap().why, LEFT_OUT);
    }

    #[test]
    fn a_preferred_provider_is_asked_first() {
        let a = Fakes::new();
        struct Other(Fakes);
        impl Provider for Other {
            fn id(&self) -> ProviderId {
                ProviderId("other")
            }
            fn audio(&self) -> Offer<dyn AudioMixer> {
                on(self.0.audio.clone())
            }
        }
        let p = PlatformBuilder::empty()
            .with(a.provider())
            .with(Other(Fakes::new()))
            .prefer(Capability::Audio, ProviderId("other"))
            .probe();
        assert_eq!(p.by(Capability::Audio), Some(ProviderId("other")));
        assert_eq!(p.by(Capability::Wifi), Some(FAKE));
    }

    #[test]
    fn reprobe_picks_up_what_appeared() {
        use std::sync::atomic::{AtomicBool, Ordering};
        static UP: AtomicBool = AtomicBool::new(false);
        struct Later(Fakes);
        impl Provider for Later {
            fn id(&self) -> ProviderId {
                ProviderId("later")
            }
            fn window_manager(&self) -> Offer<dyn WindowManager> {
                if UP.load(Ordering::SeqCst) {
                    on(self.0.windows.clone())
                } else {
                    Some(Err("Hyprland is not running".into()))
                }
            }
        }
        let p = PlatformBuilder::empty().with(Later(Fakes::new())).probe();
        assert!(!p.has(Capability::WindowManager));
        UP.store(true, Ordering::SeqCst);
        p.reprobe_stale(std::time::Duration::from_secs(3600));
        assert!(!p.has(Capability::WindowManager), "not stale yet");
        p.reprobe(Capability::WindowManager);
        assert!(p.has(Capability::WindowManager));
    }

    #[tokio::test]
    async fn host_state_reads_the_watches_without_fetching() {
        let f = Fakes::new();
        let p = f.platform();
        f.brightness.state.will_read(Some(0.5));
        assert_eq!(p.host_state().brightness, None, "nothing fetched yet");
        p.brightness()
            .unwrap()
            .fresh(std::time::Duration::ZERO)
            .await;
        assert_eq!(p.host_state().brightness, Some(0.5));
        assert_eq!(f.brightness.state.fetches(), 1);
        let _ = p.host_state();
        assert_eq!(f.brightness.state.fetches(), 1, "reading is not fetching");
        assert_eq!(p.host_state().info.hostname, "fake");
    }

    #[test]
    fn the_fakes_pass_the_contract() {
        let f = Fakes::new();
        contract::touch(f.input.touch().as_ref());
        contract::keys(f.input.keys().as_ref());
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        rt.block_on(async {
            contract::stateful(f.audio.as_ref() as &dyn AudioMixer).await;
            contract::stateful(f.wifi.as_ref() as &dyn WifiRadio).await;
            contract::stateful(f.bluetooth.as_ref() as &dyn Bluetooth).await;
            contract::stateful(f.brightness.as_ref() as &dyn Brightness).await;
            contract::stateful(f.media.as_ref() as &dyn MediaPlayers).await;
        });
        contract::platform(&f.platform());
    }
}
