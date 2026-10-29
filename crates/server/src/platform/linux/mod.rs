//! The Linux providers: today's host code, behind the platform traits (`wado_platform`).
//!
//! Each capability is offered only when opening it works here — **opening is the probe**
//! (`CLAUDE.md` invariant 6): `/dev/uinput` opens for writing, the tool is installed, the desktop
//! display was captured. What is missing says why, in the words the person used to be shown when
//! the action failed.
//!
//! Files, one job each:
//! - `uinput.rs` — the raw `/dev/uinput` device: open, describe, emit batches, destroy on drop.
//! - `touchpad.rs` — the virtual touchpad (`TouchInjector`) and its slot logic.
//! - `keyboard.rs` — the virtual keyboard (`KeyInjector`).
//! - `wtype.rs` — typing committed text (`TextInjector`).
//! - `cmd.rs` — running a host tool with a deadline, counted.
//! - `pipewire.rs` — outputs and streams (`AudioMixer`).
//! - `viewer_sink.rs` — "This phone", a PipeWire null sink (`ViewerSink`).
//! - `mpris.rs` — media players on D-Bus (`MediaPlayers`).
//! - `backlight.rs` — the screen's backlight (`Brightness`).
//! - `nmcli.rs` — Wi-Fi through NetworkManager (`WifiRadio`).
//! - `bluetoothctl.rs` — Bluetooth through BlueZ (`Bluetooth`).
//! - `inhibit.rs` — keeping awake with `systemd-inhibit` (`PowerInhibit`).
//! - `sysinfo.rs` — what the computer is (`SysInfoSource`).

pub mod backlight;
pub mod bluetoothctl;
pub mod cmd;
pub mod inhibit;
pub mod keyboard;
pub mod mpris;
pub mod nmcli;
pub mod pipewire;
pub mod sysinfo;
pub mod touchpad;
pub mod uinput;
pub mod viewer_sink;
pub mod wtype;

use std::fs::OpenOptions;
use std::io;
use std::os::unix::fs::OpenOptionsExt;
use std::sync::{Arc, OnceLock};

use wado_platform::audio::{AudioMixer, ViewerSink};
use wado_platform::display::Brightness;
use wado_platform::input::{
    GamepadInjector, InputError, KeyInjector, Opener, TextInjector, TouchInjector,
};
use wado_platform::media::MediaPlayers;
use wado_platform::network::{Bluetooth, WifiRadio};
use wado_platform::power::PowerInhibit;
use wado_platform::registry::Offer;
use wado_platform::sysinfo::SysInfoSource;
use wado_platform::{Provider, ProviderId};

use crate::actions::detect::on_path;

/// This provider's id.
pub const LINUX: ProviderId = ProviderId("linux");

/// Shown when `/dev/uinput` cannot be opened.
pub const NO_UINPUT: &str = "The trackpad needs /dev/uinput on the computer. On NixOS: hardware.uinput.enable = true, and add yourself to the uinput group.";

/// A device error in words a person can act on.
pub fn why(e: io::Error) -> InputError {
    match e.kind() {
        io::ErrorKind::PermissionDenied | io::ErrorKind::NotFound => {
            InputError::Unavailable(NO_UINPUT.into())
        }
        _ => InputError::Failed(format!("the computer could not drive the trackpad: {e}")),
    }
}

/// `/dev/uinput` opens for writing — the probe for every virtual device. Opening the node makes
/// no device; only `UI_DEV_CREATE` does.
fn uinput() -> Result<(), String> {
    OpenOptions::new()
        .write(true)
        .custom_flags(libc::O_NONBLOCK)
        .open("/dev/uinput")
        .map(drop)
        .map_err(|e| why(e).to_string())
}

/// Offer `imp` when `probe` passed.
fn offer<T: ?Sized>(probe: Result<(), String>, imp: impl FnOnce() -> Arc<T>) -> Offer<T> {
    Some(probe.map(|()| imp()))
}

/// Every tool in `tools` is installed — else which one is missing, in words.
fn tools(tools: &[&str], what: &str) -> Result<(), String> {
    match tools.iter().find(|t| !on_path(t)) {
        None => Ok(()),
        Some(t) => Err(format!(
            "{what} needs {t}, which is not installed on this computer"
        )),
    }
}

/// Every Linux provider in one: the host's own tools and devices.
///
/// A stateful capability is one instance for the daemon's life, so probing it again keeps its
/// published state and everyone subscribed to it.
#[derive(Default)]
pub struct LinuxProvider {
    audio: OnceLock<Arc<pipewire::PipeWire>>,
    media: OnceLock<Arc<mpris::Mpris>>,
    brightness: OnceLock<Arc<backlight::Backlight>>,
    wifi: OnceLock<Arc<nmcli::NetworkManager>>,
    bluetooth: OnceLock<Arc<bluetoothctl::BlueZ>>,
    awake: OnceLock<Arc<inhibit::SystemdInhibit>>,
    sys_info: OnceLock<Arc<sysinfo::ProcSysInfo>>,
}

impl LinuxProvider {
    fn pipewire(&self) -> Arc<pipewire::PipeWire> {
        self.audio
            .get_or_init(|| Arc::new(pipewire::PipeWire::new()))
            .clone()
    }
}

impl Provider for LinuxProvider {
    fn id(&self) -> ProviderId {
        LINUX
    }

    fn touch(&self) -> Offer<dyn Opener<dyn TouchInjector>> {
        offer(uinput(), || {
            Arc::new(|| Ok(Box::new(touchpad::UinputTouch::default()) as Box<dyn TouchInjector>))
                as Arc<dyn Opener<dyn TouchInjector>>
        })
    }

    fn keys(&self) -> Offer<dyn Opener<dyn KeyInjector>> {
        offer(uinput(), || {
            Arc::new(|| {
                keyboard::Keyboard::new()
                    .map(|k| Box::new(k) as Box<dyn KeyInjector>)
                    .map_err(why)
            }) as Arc<dyn Opener<dyn KeyInjector>>
        })
    }

    fn gamepad(&self) -> Offer<dyn Opener<dyn GamepadInjector>> {
        offer(uinput(), wado_compositor::input::gamepad::opener)
    }

    fn text(&self) -> Offer<dyn TextInjector> {
        let probe = wtype::command().and_then(|_| {
            crate::actions::detect::on_path("wtype")
                .then_some(())
                .ok_or_else(|| wtype::NO_WTYPE.to_string())
        });
        offer(probe, || Arc::new(wtype::Wtype) as Arc<dyn TextInjector>)
    }

    fn audio(&self) -> Offer<dyn AudioMixer> {
        let probe = tools(&["pw-dump", "wpctl"], "Sound control");
        offer(probe, || self.pipewire() as Arc<dyn AudioMixer>)
    }

    fn viewer_sink(&self) -> Offer<dyn ViewerSink> {
        Some(viewer_sink::NullSink::new().map(|s| Arc::new(s) as Arc<dyn ViewerSink>))
    }

    fn media(&self) -> Offer<dyn MediaPlayers> {
        let probe = crate::actions::detect::session_bus()
            .then_some(())
            .ok_or_else(|| crate::actions::Need::Mpris.why());
        offer(probe, || {
            self.media
                .get_or_init(|| Arc::new(mpris::Mpris::new(self.pipewire().state())))
                .clone() as Arc<dyn MediaPlayers>
        })
    }

    fn brightness(&self) -> Offer<dyn Brightness> {
        offer(tools(&["brightnessctl"], "Brightness"), || {
            self.brightness
                .get_or_init(|| Arc::new(backlight::Backlight::new()))
                .clone() as Arc<dyn Brightness>
        })
    }

    fn wifi(&self) -> Offer<dyn WifiRadio> {
        offer(tools(&["nmcli"], "Wi-Fi"), || {
            self.wifi
                .get_or_init(|| Arc::new(nmcli::NetworkManager::new()))
                .clone() as Arc<dyn WifiRadio>
        })
    }

    fn bluetooth(&self) -> Offer<dyn Bluetooth> {
        offer(tools(&["bluetoothctl"], "Bluetooth"), || {
            self.bluetooth
                .get_or_init(|| Arc::new(bluetoothctl::BlueZ::new()))
                .clone() as Arc<dyn Bluetooth>
        })
    }

    fn power_inhibit(&self) -> Offer<dyn PowerInhibit> {
        offer(tools(&["systemd-inhibit"], "Keeping awake"), || {
            self.awake
                .get_or_init(|| Arc::new(inhibit::SystemdInhibit::new()))
                .clone() as Arc<dyn PowerInhibit>
        })
    }

    fn sys_info(&self) -> Offer<dyn SysInfoSource> {
        offer(Ok(()), || {
            self.sys_info
                .get_or_init(|| Arc::new(sysinfo::ProcSysInfo::new()))
                .clone() as Arc<dyn SysInfoSource>
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The host tools pass the contract: a refresh is bounded, and an ask right after one is
    /// answered from it. `cargo test -p wado platform::linux -- --ignored`.
    #[tokio::test]
    #[ignore = "runs the host's tools"]
    async fn the_host_tools_pass_the_contract() {
        use wado_platform::fake::contract::stateful;
        let p = wado_platform::PlatformBuilder::empty()
            .with(LinuxProvider::default())
            .probe();
        if let Ok(a) = p.audio() {
            stateful(a.as_ref()).await;
        }
        if let Ok(m) = p.media() {
            stateful(m.as_ref()).await;
        }
        if let Ok(b) = p.brightness() {
            stateful(b.as_ref()).await;
        }
        if let Ok(w) = p.wifi() {
            stateful(w.as_ref()).await;
        }
        if let Ok(b) = p.bluetooth() {
            stateful(b.as_ref()).await;
        }
        eprintln!("{:#?}", p.caps());
    }

    /// The platform contract, on the real kernel devices:
    /// `cargo test -p wado platform::linux -- --ignored` with the sandbox off.
    #[test]
    #[ignore = "needs /dev/uinput"]
    fn the_uinput_devices_pass_the_contract() {
        let p = wado_platform::PlatformBuilder::empty()
            .with(LinuxProvider::default())
            .probe();
        wado_platform::fake::contract::touch(p.touch().unwrap().as_ref());
        wado_platform::fake::contract::keys(p.keys().unwrap().as_ref());
        wado_platform::fake::contract::platform(&p);
    }
}
