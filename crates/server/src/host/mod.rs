//! The computer itself, controlled from the viewer: its sound, Wi-Fi, Bluetooth, sleep.
//!
//! Works with no session running — only the relay link is needed — so a phone with just a
//! shell open can still turn the computer's music down or move it to the phone.
//!
//! - [`phone_sink`] — the "This phone" output.
//! - [`audio`] — outputs and streams (PipeWire / WirePlumber).
//! - [`wifi`] — NetworkManager, with changes on probation.
//! - [`bluetooth`] — BlueZ.
//! - [`awake`] — a sleep inhibitor.
//! - [`cmd`] — running the tools.

pub mod audio;
pub mod awake;
pub mod bluetooth;
pub mod cmd;
pub mod media;
pub mod phone_sink;
pub mod sysinfo;
pub mod wifi;

use std::sync::OnceLock;

use wado_protocol::{HostAction, HostState};

static PHONE: OnceLock<Option<String>> = OnceLock::new();
static SINK: std::sync::Mutex<Option<phone_sink::AudioSink>> = std::sync::Mutex::new(None);

/// Create the phone sink, once per daemon. Returns its name.
pub fn start() -> Option<String> {
    PHONE
        .get_or_init(|| {
            let sink = phone_sink::start()?;
            let name = sink.name.clone();
            *SINK.lock().unwrap_or_else(|e| e.into_inner()) = Some(sink);
            Some(name)
        })
        .clone()
}

pub fn phone() -> Option<String> {
    PHONE.get().cloned().flatten()
}

pub async fn state() -> HostState {
    let phone = phone();
    let (audio, wifi, bluetooth) = tokio::join!(
        audio::state(phone.as_deref()),
        wifi::state(),
        bluetooth::state()
    );
    let media = media::state(&audio.streams).await;
    HostState {
        media,
        info: sysinfo::now(),
        audio,
        wifi,
        bluetooth,
        awake: awake::on(),
        wifi_revert_in: wifi::revert_in(),
    }
}

pub async fn act(action: HostAction) -> Result<(), String> {
    match action {
        HostAction::SinkVolume { id, volume } | HostAction::StreamVolume { id, volume } => {
            audio::set_volume(id, volume).await
        }
        HostAction::SinkMute { id, muted } | HostAction::StreamMute { id, muted } => {
            audio::set_mute(id, muted).await
        }
        HostAction::DefaultSink { name } => {
            let a = audio::state(None).await;
            audio::set_default(&name, &a.sinks).await
        }
        HostAction::StreamTo { id, sink } => audio::move_stream(id, &sink).await,
        HostAction::AllTo { sink } => audio::all_to(&sink, &audio::state(None).await).await,
        HostAction::WifiRadio { on, minutes } => wifi::radio(on, minutes).await,
        HostAction::WifiConnect { ssid, password } => {
            wifi::connect(&ssid, password.as_deref()).await
        }
        HostAction::WifiKeep => {
            wifi::keep();
            Ok(())
        }
        HostAction::BtPower { on } => bluetooth::power(on).await,
        HostAction::BtConnect { addr, connect } => bluetooth::connect(&addr, connect).await,
        HostAction::KeepAwake { on } => awake::set(on),
        HostAction::Media { bus, op } => media::act(&bus, op).await,
    }
}
