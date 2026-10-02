//! Bluetooth, through BlueZ's `bluetoothctl`: power, and the paired devices.
//!
//! ponytail: paired devices only. Pairing a new one needs the device in reach of the computer
//! and usually a PIN on both, so it stays a job for the computer's own settings.

use wado_protocol::host::{Bluetooth, BtDevice};

use super::cmd::run;

pub async fn state() -> Option<Bluetooth> {
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
    Some(Bluetooth { powered, devices })
}

pub async fn power(on: bool) -> Result<(), String> {
    run("bluetoothctl", &["power", if on { "on" } else { "off" }])
        .await
        .map(drop)
}

/// Only an address `bluetoothctl` itself listed can be named here.
pub async fn connect(addr: &str, connect: bool) -> Result<(), String> {
    if !(addr.len() == 17 && addr.chars().all(|c| c.is_ascii_hexdigit() || c == ':')) {
        return Err("not a Bluetooth address".into());
    }
    run(
        "bluetoothctl",
        &[if connect { "connect" } else { "disconnect" }, addr],
    )
    .await
    .map(drop)
}
