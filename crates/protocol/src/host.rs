//! The computer running the daemon, as the control centre shows it: its sound, Wi-Fi and
//! Bluetooth, and whether it is being kept awake. Readable and controllable over the relay with
//! no session running — a shell-only connection can turn the computer's volume down.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct HostState {
    #[serde(default)]
    pub audio: HostAudio,
    /// `None`: no NetworkManager on this computer.
    #[serde(default)]
    pub wifi: Option<Wifi>,
    /// `None`: no BlueZ, or no adapter.
    #[serde(default)]
    pub bluetooth: Option<Bluetooth>,
    /// The computer is being kept from sleeping by this daemon.
    #[serde(default)]
    pub awake: bool,
    /// A Wi-Fi change is on probation: seconds until it is undone unless someone confirms it.
    #[serde(default)]
    pub wifi_revert_in: Option<u32>,
    /// Media players (MPRIS) on the computer and in the session — playing ones first.
    #[serde(default)]
    pub media: Vec<Player>,
    /// What the computer is, neofetch-style. The hostname is what a device card is named after
    /// until the person names it.
    #[serde(default)]
    pub info: SysInfo,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SysInfo {
    pub user: String,
    pub hostname: String,
    /// `PRETTY_NAME` from os-release, e.g. "NixOS 26.11 (Xantusia)".
    pub os: String,
    /// os-release `ID`, e.g. "nixos" — picks the logo.
    pub os_id: String,
    pub kernel: String,
    pub uptime_s: u64,
    pub cpu: String,
    pub cores: u32,
    pub gpu: String,
    pub mem_used_mb: u64,
    pub mem_total_mb: u64,
    pub shell: String,
    pub desktop: String,
}

/// A media player, as its playback card shows it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Player {
    /// Its D-Bus name — what an action addresses.
    pub bus: String,
    /// "Spotify", "Firefox".
    pub app: String,
    pub title: String,
    pub artist: String,
    /// Cover art: an `https:` URL as the player gives it, or a `data:` URI for a local file.
    #[serde(default)]
    pub art: Option<String>,
    pub playing: bool,
    pub position_ms: u64,
    /// 0 when the player does not say (a live stream).
    pub length_ms: u64,
    pub shuffle: Option<bool>,
    pub can_next: bool,
    pub can_prev: bool,
    pub can_seek: bool,
    /// The audio stream it plays through, when it could be matched — what the card's output
    /// chip shows and moves.
    #[serde(default)]
    pub stream: Option<u32>,
    /// The app's own icon, as a `data:` URI, when its desktop entry names one.
    #[serde(default)]
    pub icon: Option<String>,
    /// MPRIS `LoopStatus`: `None`, `Track` or `Playlist`; absent when the player has none.
    #[serde(default)]
    pub loop_status: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum MediaOp {
    PlayPause,
    Next,
    Previous,
    SeekTo {
        ms: u64,
    },
    Shuffle {
        on: bool,
    },
    /// `None`, `Track` or `Playlist`.
    Loop {
        mode: String,
    },
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct HostAudio {
    pub sinks: Vec<Sink>,
    pub streams: Vec<Stream>,
    /// `node.name` of the default output.
    pub default_sink: String,
    /// `node.name` of the sink that plays on the viewer — "This phone".
    pub phone_sink: Option<String>,
}

/// An output: speakers, headphones, HDMI, a Bluetooth headset — or the viewer.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Sink {
    pub id: u32,
    /// `node.name`, what routing names it by.
    pub name: String,
    /// What a person calls it.
    pub label: String,
    /// 0..1 is the slider; above 1 is boost.
    pub volume: f32,
    pub muted: bool,
}

/// Something playing.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Stream {
    pub id: u32,
    pub app: String,
    /// What it is playing, when it says.
    pub title: String,
    pub volume: f32,
    pub muted: bool,
    /// `node.name` of the output it is playing on now.
    pub sink: Option<String>,
    /// The process playing it, which is what ties a media player to its stream.
    #[serde(default)]
    pub pid: Option<u32>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Wifi {
    pub enabled: bool,
    /// The network in use, if any.
    pub connected: Option<String>,
    pub networks: Vec<Network>,
    /// This computer reaches the internet through Wi-Fi alone — changing it can cut the very
    /// connection the viewer is using.
    pub uplink: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Network {
    pub ssid: String,
    /// 0..100.
    pub signal: u8,
    pub secure: bool,
    pub active: bool,
    /// A saved connection exists, so no password is needed.
    pub known: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Bluetooth {
    pub powered: bool,
    /// Paired devices.
    pub devices: Vec<BtDevice>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct BtDevice {
    pub addr: String,
    pub name: String,
    pub connected: bool,
    /// BlueZ's icon name: `audio-headset`, `input-gaming`, …
    pub icon: String,
}

/// Something to do to the computer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "do", rename_all = "snake_case")]
pub enum HostAction {
    SinkVolume {
        id: u32,
        volume: f32,
    },
    SinkMute {
        id: u32,
        muted: bool,
    },
    DefaultSink {
        name: String,
    },
    StreamVolume {
        id: u32,
        volume: f32,
    },
    StreamMute {
        id: u32,
        muted: bool,
    },
    /// Move one stream to an output.
    StreamTo {
        id: u32,
        sink: String,
    },
    /// Everything — what is playing now and what plays next — to one output.
    AllTo {
        sink: String,
    },
    /// Wi-Fi on or off. Off with `minutes` comes back on by itself after that long.
    WifiRadio {
        on: bool,
        minutes: Option<u32>,
    },
    /// Join a network. Undone after a minute unless confirmed with `WifiKeep`.
    WifiConnect {
        ssid: String,
        password: Option<String>,
    },
    WifiKeep,
    BtPower {
        on: bool,
    },
    BtConnect {
        addr: String,
        connect: bool,
    },
    KeepAwake {
        on: bool,
    },
    /// Control a media player.
    Media {
        bus: String,
        op: MediaOp,
    },
}

impl HostAction {
    /// Can cut this computer's connection, so only the owner device may do it.
    pub fn is_network(&self) -> bool {
        matches!(
            self,
            HostAction::WifiRadio { .. } | HostAction::WifiConnect { .. } | HostAction::WifiKeep
        )
    }
}
