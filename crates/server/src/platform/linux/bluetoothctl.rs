//! Bluetooth ([`Bluetooth`] on Linux), through BlueZ's `bluetoothctl`: power, and the paired
//! devices.
//!
//! ponytail: paired devices only. Pairing a new one needs the device in reach of the computer
//! and usually a PIN on both, so it stays a job for the computer's own settings.

use std::time::Duration;

use tokio::sync::watch;
use wado_platform::network::Bluetooth;
use wado_platform::{Fut, Published, Res};
use wado_protocol::host::{Bluetooth as BtState, BtDevice};

use super::cmd::run;

/// BlueZ through `bluetoothctl`.
pub struct BlueZ {
    state: Published<Option<BtState>>,
}

impl BlueZ {
    pub fn new() -> BlueZ {
        BlueZ {
            state: Published::new(None),
        }
    }
}

impl Bluetooth for BlueZ {
    fn state(&self) -> watch::Receiver<Option<BtState>> {
        self.state.subscribe()
    }

    fn fresh(&self, within: Duration) -> Fut<'_, ()> {
        Box::pin(self.state.fresh(within, read))
    }

    fn power(&self, on: bool) -> Fut<'_, Res<()>> {
        Box::pin(async move {
            run("bluetoothctl", &["power", if on { "on" } else { "off" }])
                .await
                .map(drop)
        })
    }

    fn connect(&self, addr: String, connect: bool) -> Fut<'_, Res<()>> {
        Box::pin(async move {
            run(
                "bluetoothctl",
                &[if connect { "connect" } else { "disconnect" }, &addr],
            )
            .await
            .map(drop)
        })
    }
}

/// The adapter and its paired devices, or `None` with no adapter.
async fn read() -> Option<BtState> {
    let show = run("bluetoothctl", &["show"]).await.ok()?;
    if !show.contains("Controller") {
        return None;
    }
    let powered = show.lines().any(|l| l.trim() == "Powered: yes");
    let mut devices = Vec::new();
    for line in run("bluetoothctl", &["devices", "Paired"])
        .await
        .unwrap_or_default()
        .lines()
    {
        let mut w = line.splitn(3, ' ');
        let (Some("Device"), Some(addr), Some(name)) = (w.next(), w.next(), w.next()) else {
            continue;
        };
        let info = run("bluetoothctl", &["info", addr])
            .await
            .unwrap_or_default();
        let field = |k: &str| {
            info.lines()
                .find_map(|l| l.trim().strip_prefix(k).map(|v| v.trim().to_string()))
        };
        devices.push(BtDevice {
            addr: addr.into(),
            name: name.into(),
            connected: field("Connected:").as_deref() == Some("yes"),
            icon: field("Icon:").unwrap_or_default(),
        });
    }
    devices.sort_by(|a, b| b.connected.cmp(&a.connected));
    Some(BtState { powered, devices })
}
