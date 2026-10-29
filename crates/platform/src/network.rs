//! Wi-Fi and Bluetooth. The mechanisms only: putting a Wi-Fi change on probation (undone unless a
//! viewer confirms it) is the caller's policy, written once for every platform.

use tokio::sync::watch;
use wado_protocol::host::{Bluetooth as BtState, Wifi};

use crate::{Fut, Res};

/// Wi-Fi. State `None`: no Wi-Fi manager here.
pub trait WifiRadio: Send + Sync {
    fn state(&self) -> watch::Receiver<Option<Wifi>>;
    fn fresh(&self, within: std::time::Duration) -> Fut<'_, ()>;
    /// The radio on or off.
    fn radio(&self, on: bool) -> Fut<'_, Res<()>>;
    /// Join `ssid` — a known network by its saved settings when no password is given.
    fn join(&self, ssid: String, password: Option<String>) -> Fut<'_, Res<()>>;
    /// Bring back the saved network `name` (going back after a change nobody confirmed).
    fn rejoin(&self, name: String) -> Fut<'_, Res<()>>;
}

/// Bluetooth: power, and the paired devices. State `None`: no Bluetooth stack or adapter.
pub trait Bluetooth: Send + Sync {
    fn state(&self) -> watch::Receiver<Option<BtState>>;
    fn fresh(&self, within: std::time::Duration) -> Fut<'_, ()>;
    fn power(&self, on: bool) -> Fut<'_, Res<()>>;
    /// Connect or disconnect the paired device at `addr` (`AA:BB:CC:DD:EE:FF`, checked by the
    /// caller).
    fn connect(&self, addr: String, connect: bool) -> Fut<'_, Res<()>>;
}
