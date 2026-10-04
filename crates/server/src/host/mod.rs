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
pub mod trim;
pub mod wifi;

use std::sync::OnceLock;

use wado_protocol::{HostAction, HostState};

static PHONE: OnceLock<Option<String>> = OnceLock::new();
/// The viewer's sink while one is connected, with the description it was created under.
static SINK: std::sync::Mutex<Option<(phone_sink::AudioSink, String)>> =
    std::sync::Mutex::new(None);

/// The viewer sink's node name for this daemon run (the sink itself exists only while a device
/// is connected — see [`viewer`]).
pub fn start() -> Option<String> {
    PHONE.get_or_init(phone_sink::name).clone()
}

/// A device connected (`Some(name)`) or the last one went (`None`).
///
/// Connected: the sink exists, described after the device. A different device than the sink was
/// made for gets it recreated under its own name — PipeWire cannot rename a node — and the apps
/// that were playing to it are moved back onto it. Gone: the sink is removed.
pub async fn viewer(device: Option<String>) {
    let Some(name) = start() else { return };
    let Some(device) = device else {
        if let Some((sink, _)) = SINK.lock().unwrap_or_else(|e| e.into_inner()).take() {
            phone_sink::terminate(sink);
            tracing::info!(
                sink = name,
                "viewer audio sink removed — no device connected"
            );
        }
        return;
    };
    let description = format!(
        "wado · {}",
        if device.is_empty() {
            "viewer"
        } else {
            device.as_str()
        }
    );
    if SINK
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .is_some_and(|(_, d)| *d == description)
    {
        return;
    }
    // Who plays to it now, to put back after the swap.
    let playing: Vec<u32> = audio::state(Some(&name))
        .await
        .streams
        .iter()
        .filter(|s| s.sink.as_deref() == Some(name.as_str()))
        .map(|s| s.id)
        .collect();
    if let Some((old, _)) = SINK.lock().unwrap_or_else(|e| e.into_inner()).take() {
        phone_sink::terminate(old);
    }
    let Some(sink) = phone_sink::start(&name, &description) else {
        return;
    };
    *SINK.lock().unwrap_or_else(|e| e.into_inner()) = Some((sink, description));
    if !playing.is_empty() {
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        for id in playing {
            let _ = audio::move_stream(id, &name).await;
        }
    }
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
