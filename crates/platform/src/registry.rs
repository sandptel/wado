//! The registry: which provider gives each capability on this computer, or why none does.
//!
//! Providers are registered in order, each offering some capabilities. [`PlatformBuilder::probe`]
//! opens each capability from the first provider that can — **opening is the probe**: a tool's
//! presence or a driver string proves nothing, so a provider only offers what it actually
//! opened. The result is cached; [`Platform::reprobe`] looks again (a tool installed, a desktop
//! restarted, a device plugged in).
//!
//! ```ignore
//! let platform = PlatformBuilder::empty()
//!     .with(LinuxProvider::new())
//!     .with(Hyprland::new())
//!     .prefer(Capability::Audio, ProviderId("pipewire"))
//!     .without(Capability::Gamepad)
//!     .probe();
//! let audio = platform.audio()?; // Arc<dyn AudioMixer>, or Absent { why, tried }
//! ```

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use wado_protocol::HostState;
use wado_protocol::caps::{CapOff, CapSet, Capability};

use crate::audio::{AudioMixer, ViewerSink};
use crate::cap::{Absent, Cap, ProviderId};
use crate::clip::{ClipHistory, Clipboard};
use crate::desktop::{Launcher, Screenshot, WindowManager};
use crate::display::Brightness;
use crate::files::{Thumbnailer, Trash};
use crate::input::{GamepadInjector, KeyInjector, Opener, TextInjector, TouchInjector};
use crate::media::MediaPlayers;
use crate::network::{Bluetooth, WifiRadio};
use crate::power::{PowerInhibit, SystemPower};
use crate::pty::PtySpawner;
use crate::sysinfo::SysInfoSource;

/// What a provider says when asked for a capability: not offered (`None`), opened, or tried and
/// failed with the reason.
pub type Offer<T> = Option<Result<Arc<T>, String>>;

/// Shown for a capability no registered provider offers.
pub const NOBODY: &str = "nothing on this computer provides it";
/// Shown for a capability left out with [`PlatformBuilder::without`].
pub const LEFT_OUT: &str = "turned off for this daemon";

macro_rules! registry {
    ($( $field:ident : $cap:ident => $ty:ty ),* $(,)?) => {
        /// A source of capabilities: one platform's backend, or one part of it. Every method is
        /// "open this capability": `None` when not offered, `Some(Err(why))` when it was tried
        /// here and is not available.
        pub trait Provider: Send + Sync {
            fn id(&self) -> ProviderId;
            $(
                fn $field(&self) -> Offer<$ty> {
                    None
                }
            )*
        }

        /// Every capability's slot.
        struct Slots {
            $( $field: RwLock<Cap<$ty>>, )*
        }

        impl Platform {
            $(
                #[doc = concat!("The `", stringify!($cap), "` capability, or why it is absent.")]
                pub fn $field(&self) -> Result<Arc<$ty>, Absent> {
                    read(&self.slots.$field).get()
                }
            )*

            /// Open `cap` again from the providers, replacing what was cached.
            pub fn reprobe(&self, cap: Capability) {
                match cap {
                    $(
                        Capability::$cap => {
                            let found = self.resolve(cap, |p| p.$field());
                            *self.slots.$field.write().unwrap_or_else(|e| e.into_inner()) = found;
                        }
                    )*
                }
            }

            /// Whether `cap` is provided, who by, and why not.
            fn status(&self, cap: Capability) -> (Option<ProviderId>, Option<Absent>) {
                match cap {
                    $(
                        Capability::$cap => match &*read(&self.slots.$field) {
                            Cap::On { by, .. } => (Some(*by), None),
                            Cap::Off(a) => (None, Some(a.clone())),
                        },
                    )*
                }
            }

            fn unprobed() -> Slots {
                Slots {
                    $( $field: RwLock::new(Cap::Off(Absent::because("not probed yet"))), )*
                }
            }
        }
    };
}

registry! {
    touch: Touch => dyn Opener<dyn TouchInjector>,
    keys: Keys => dyn Opener<dyn KeyInjector>,
    gamepad: Gamepad => dyn Opener<dyn GamepadInjector>,
    text: Text => dyn TextInjector,
    audio: Audio => dyn AudioMixer,
    viewer_sink: ViewerSink => dyn ViewerSink,
    media: Media => dyn MediaPlayers,
    brightness: Brightness => dyn Brightness,
    wifi: Wifi => dyn WifiRadio,
    bluetooth: Bluetooth => dyn Bluetooth,
    power_inhibit: PowerInhibit => dyn PowerInhibit,
    system_power: SystemPower => dyn SystemPower,
    clipboard: Clipboard => dyn Clipboard,
    clip_history: ClipHistory => dyn ClipHistory,
    window_manager: WindowManager => dyn WindowManager,
    launcher: Launcher => dyn Launcher,
    screenshot: Screenshot => dyn Screenshot,
    sys_info: SysInfo => dyn SysInfoSource,
    trash: Trash => dyn Trash,
    thumbnailer: Thumbnailer => dyn Thumbnailer,
    pty: Pty => dyn PtySpawner,
}

fn read<T>(l: &RwLock<T>) -> std::sync::RwLockReadGuard<'_, T> {
    l.read().unwrap_or_else(|e| e.into_inner())
}

/// This computer's capabilities, probed once and cached.
pub struct Platform {
    providers: Vec<Arc<dyn Provider>>,
    prefer: HashMap<Capability, ProviderId>,
    without: HashSet<Capability>,
    slots: Slots,
}

impl Platform {
    /// `cap` is provided here.
    pub fn has(&self, cap: Capability) -> bool {
        self.status(cap).0.is_some()
    }

    /// Who provides `cap`.
    pub fn by(&self, cap: Capability) -> Option<ProviderId> {
        self.status(cap).0
    }

    /// Why `cap` is missing, when it is.
    pub fn absent(&self, cap: Capability) -> Option<Absent> {
        self.status(cap).1
    }

    /// Probe again every capability found missing more than `older` ago — what turns a tool
    /// installed, or a desktop started, into a button without a daemon restart.
    pub fn reprobe_stale(&self, older: Duration) {
        for cap in Capability::ALL {
            if self.absent(cap).is_some_and(|a| a.at.elapsed() > older) {
                self.reprobe(cap);
            }
        }
    }

    /// What is here and what is not, for the clients to hide what is missing.
    pub fn caps(&self) -> CapSet {
        let mut set = CapSet::default();
        for cap in Capability::ALL {
            match self.status(cap) {
                (Some(_), _) => set.on.push(cap.name().to_string()),
                (None, a) => set.off.push(CapOff {
                    cap: cap.name().to_string(),
                    why: a.map(|a| a.why).unwrap_or_default(),
                }),
            }
        }
        set
    }

    /// The computer's state as the wire carries it, from what each capability last published —
    /// no I/O. Missing capabilities read as the wire's "not here" (`None`, empty, `false`).
    /// Viewer-specific decoration (which output is "This phone", a Wi-Fi change's countdown) is
    /// the caller's.
    pub fn host_state(&self) -> HostState {
        HostState {
            audio: self
                .audio()
                .map(|a| a.state().borrow().clone())
                .unwrap_or_default(),
            wifi: self.wifi().ok().and_then(|w| w.state().borrow().clone()),
            bluetooth: self
                .bluetooth()
                .ok()
                .and_then(|b| b.state().borrow().clone()),
            awake: self.power_inhibit().is_ok_and(|p| {
                p.check();
                *p.state().borrow()
            }),
            brightness: self.brightness().ok().and_then(|b| *b.state().borrow()),
            wifi_revert_in: None,
            media: self
                .media()
                .map(|m| m.state().borrow().clone())
                .unwrap_or_default(),
            info: self.sys_info().map(|s| s.now()).unwrap_or_default(),
        }
    }

    /// `cap` from the first provider that opens it — the preferred one first — or why none did.
    fn resolve<T: ?Sized>(
        &self,
        cap: Capability,
        offer: impl Fn(&dyn Provider) -> Offer<T>,
    ) -> Cap<T> {
        if self.without.contains(&cap) {
            return Cap::Off(Absent::because(LEFT_OUT));
        }
        let first = self.prefer.get(&cap).copied();
        let ordered = self
            .providers
            .iter()
            .filter(|p| Some(p.id()) == first)
            .chain(self.providers.iter().filter(|p| Some(p.id()) != first));
        let mut tried = Vec::new();
        for p in ordered {
            match offer(p.as_ref()) {
                None => {}
                Some(Ok(imp)) => return Cap::On { imp, by: p.id() },
                Some(Err(why)) => tried.push((p.id(), why)),
            }
        }
        let mut absent = Absent::because(tried.last().map_or(NOBODY, |(_, why)| why.as_str()));
        absent.tried = tried;
        Cap::Off(absent)
    }
}

/// Builds a [`Platform`] from providers, then probes it.
#[derive(Default)]
pub struct PlatformBuilder {
    providers: Vec<Arc<dyn Provider>>,
    prefer: HashMap<Capability, ProviderId>,
    without: HashSet<Capability>,
}

impl PlatformBuilder {
    /// No providers: every capability absent until some are added. (A platform's default set
    /// is built by its backend crate, which knows its providers.)
    pub fn empty() -> Self {
        Self::default()
    }

    /// Add a provider, after those already added.
    pub fn with(mut self, provider: impl Provider + 'static) -> Self {
        self.providers.push(Arc::new(provider));
        self
    }

    /// Ask `by` first for `cap`, whatever the order.
    pub fn prefer(mut self, cap: Capability, by: ProviderId) -> Self {
        self.prefer.insert(cap, by);
        self
    }

    /// Leave `cap` out, even where it could be provided.
    pub fn without(mut self, cap: Capability) -> Self {
        self.without.insert(cap);
        self
    }

    /// Open every capability, and cache what was found.
    pub fn probe(self) -> Platform {
        let p = Platform {
            providers: self.providers,
            prefer: self.prefer,
            without: self.without,
            slots: Platform::unprobed(),
        };
        for cap in Capability::ALL {
            p.reprobe(cap);
        }
        p
    }
}
