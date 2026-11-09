//! The macOS providers: this Mac's capabilities behind `wado-platform`'s traits.
//!
//! Each capability is offered only when opening it works here (invariant 6); what is missing
//! says why, naming the System Settings pane when a permission is the reason.
#![cfg(target_os = "macos")]

pub mod art;
pub mod audio;
pub mod brightness;
pub mod clip;
pub mod cmd;
pub mod files;
pub mod input;
pub mod launch;
pub mod media;
pub mod nowplaying;
pub mod power;
pub mod sysinfo;
pub mod tcc;

use std::path::Path;
use std::sync::{Arc, OnceLock};

use wado_platform::audio::AudioMixer;
use wado_platform::clip::Clipboard;
use wado_platform::desktop::{Launcher, Screenshot};
use wado_platform::display::Brightness;
use wado_platform::files::{Thumbnailer, Trash};
use wado_platform::input::{
    GamepadInjector, InputError, KeyInjector, Opener, TextInjector, TouchInjector,
};
use wado_platform::media::MediaPlayers;
use wado_platform::power::{PowerInhibit, SystemPower};
use wado_platform::sysinfo::SysInfoSource;
use wado_platform::{Offer, Provider, ProviderId};

pub const MACOS: ProviderId = ProviderId("macos");

/// An offer that is always here.
fn here<T: ?Sized>(imp: Arc<T>) -> Offer<T> {
    Some(Ok(imp))
}

/// An offer that is here when `probe` passes.
fn when<T: ?Sized>(probe: Result<(), String>, imp: impl FnOnce() -> Arc<T>) -> Offer<T> {
    Some(probe.map(|()| imp()))
}

/// An input device: offered while Accessibility is granted, and asked again at each open, as a
/// grant can be withdrawn.
fn device<D: ?Sized + 'static>(open: fn() -> Result<Box<D>, InputError>) -> Offer<dyn Opener<D>> {
    when(tcc::need_accessibility(), || {
        Arc::new(move || {
            tcc::need_accessibility().map_err(InputError::Unavailable)?;
            open()
        }) as Arc<dyn Opener<D>>
    })
}

/// Every macOS provider in one. A stateful capability is one instance for the daemon's life.
#[derive(Default)]
pub struct MacProvider {
    awake: OnceLock<Arc<power::MacAwake>>,
    clipboard: OnceLock<Arc<clip::MacClipboard>>,
    audio: OnceLock<Arc<audio::MacAudio>>,
    media: OnceLock<Arc<media::MacMedia>>,
    brightness: OnceLock<Arc<brightness::MacBrightness>>,
}

impl Provider for MacProvider {
    fn id(&self) -> ProviderId {
        MACOS
    }

    fn touch(&self) -> Offer<dyn Opener<dyn TouchInjector>> {
        device(|| input::MacTouch::open().map(|t| Box::new(t) as Box<dyn TouchInjector>))
    }

    fn keys(&self) -> Offer<dyn Opener<dyn KeyInjector>> {
        device(|| {
            Ok(Box::new(input::keys::MacKeys::new(
                input::cg::CgPoster::new(),
            )))
        })
    }

    fn gamepad(&self) -> Offer<dyn Opener<dyn GamepadInjector>> {
        device(|| {
            Ok(Box::new(input::gamepad::PadKeys::new(
                input::cg::CgPoster::new(),
            )))
        })
    }

    fn text(&self) -> Offer<dyn TextInjector> {
        when(tcc::need_accessibility(), || {
            Arc::new(input::keys::MacText) as Arc<dyn TextInjector>
        })
    }

    fn clipboard(&self) -> Offer<dyn Clipboard> {
        here(
            self.clipboard
                .get_or_init(|| Arc::new(clip::MacClipboard::new()))
                .clone(),
        )
    }

    fn trash(&self) -> Offer<dyn Trash> {
        here(Arc::new(files::MacTrash::new()))
    }

    fn thumbnailer(&self) -> Offer<dyn Thumbnailer> {
        let probe = Path::new("/usr/bin/qlmanage")
            .exists()
            .then_some(())
            .ok_or_else(|| "this Mac has no Quick Look (qlmanage)".to_string());
        when(probe, || Arc::new(files::MacThumbs) as Arc<dyn Thumbnailer>)
    }

    fn audio(&self) -> Offer<dyn AudioMixer> {
        here(
            self.audio
                .get_or_init(|| Arc::new(audio::MacAudio::new()))
                .clone(),
        )
    }

    fn media(&self) -> Offer<dyn MediaPlayers> {
        here(
            self.media
                .get_or_init(|| Arc::new(media::MacMedia::new()))
                .clone(),
        )
    }

    fn brightness(&self) -> Offer<dyn Brightness> {
        if let Some(b) = self.brightness.get() {
            return here(b.clone());
        }
        Some(
            brightness::MacBrightness::probe().map(|b| {
                self.brightness.get_or_init(|| Arc::new(b)).clone() as Arc<dyn Brightness>
            }),
        )
    }

    fn sys_info(&self) -> Offer<dyn SysInfoSource> {
        here(Arc::new(sysinfo::MacSysInfo))
    }

    fn power_inhibit(&self) -> Offer<dyn PowerInhibit> {
        here(
            self.awake
                .get_or_init(|| Arc::new(power::MacAwake::new()))
                .clone(),
        )
    }

    fn system_power(&self) -> Offer<dyn SystemPower> {
        here(Arc::new(power::MacPower))
    }

    fn launcher(&self) -> Offer<dyn Launcher> {
        here(Arc::new(launch::MacLauncher))
    }

    fn screenshot(&self) -> Offer<dyn Screenshot> {
        let probe = tcc::screen_recording()
            .then_some(())
            .ok_or_else(|| tcc::NEEDS_SCREEN_RECORDING.to_string());
        when(probe, || {
            Arc::new(launch::MacScreenshot) as Arc<dyn Screenshot>
        })
    }
}
