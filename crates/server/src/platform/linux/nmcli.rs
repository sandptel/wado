//! Wi-Fi ([`WifiRadio`] on Linux), through NetworkManager's `nmcli`.
//!
//! Only the mechanisms. Putting a change on probation — undone unless a viewer confirms it,
//! because turning Wi-Fi off can cut the very link the viewer is on — is the daemon's policy
//! (`crate::host::wifi`), the same on every platform.

use std::time::Duration;

use tokio::sync::watch;
use wado_platform::network::WifiRadio;
use wado_platform::{Fut, Published, Res};
use wado_protocol::host::{Network, Wifi};

use super::cmd::run;

/// NetworkManager through `nmcli`.
pub struct NetworkManager {
    state: Published<Option<Wifi>>,
}

impl NetworkManager {
    pub fn new() -> NetworkManager {
        NetworkManager {
            state: Published::new(None),
        }
    }
}

impl WifiRadio for NetworkManager {
    fn state(&self) -> watch::Receiver<Option<Wifi>> {
        self.state.subscribe()
    }

    fn fresh(&self, within: Duration) -> Fut<'_, ()> {
        Box::pin(self.state.fresh(within, read))
    }

    fn radio(&self, on: bool) -> Fut<'_, Res<()>> {
        Box::pin(async move {
            run("nmcli", &["radio", "wifi", if on { "on" } else { "off" }])
                .await
                .map(drop)
        })
    }

    /// A password joins as a new network; without one, the saved connection of that name, else
    /// an open network.
    fn join(&self, ssid: String, password: Option<String>) -> Fut<'_, Res<()>> {
        Box::pin(async move {
            match password.filter(|p| !p.is_empty()) {
                Some(pw) => run("nmcli", &["dev", "wifi", "connect", &ssid, "password", &pw]).await,
                None => match run("nmcli", &["connection", "up", "id", &ssid]).await {
                    Ok(o) => Ok(o),
                    Err(_) => run("nmcli", &["dev", "wifi", "connect", &ssid]).await,
                },
            }
            .map(drop)
        })
    }

    fn rejoin(&self, name: String) -> Fut<'_, Res<()>> {
        Box::pin(async move {
            run("nmcli", &["connection", "up", "id", &name])
                .await
                .map(drop)
        })
    }
}

/// The radio, the networks in reach, and whether the internet goes through Wi-Fi alone — or
/// `None` without NetworkManager.
async fn read() -> Option<Wifi> {
    let radio = run("nmcli", &["-t", "-f", "WIFI", "radio"]).await.ok()?;
    let enabled = radio.trim() == "enabled";
    let known: Vec<String> = run("nmcli", &["-t", "-f", "NAME,TYPE", "connection", "show"])
        .await
        .unwrap_or_default()
        .lines()
        .filter_map(|l| {
            l.rsplit_once(':')
                .filter(|(_, t)| t.contains("wireless"))
                .map(|(n, _)| unescape(n))
        })
        .collect();
    let mut networks: Vec<Network> = Vec::new();
    if enabled {
        let list = run(
            "nmcli",
            &[
                "-t",
                "-f",
                "IN-USE,SSID,SIGNAL,SECURITY",
                "dev",
                "wifi",
                "list",
                "--rescan",
                "no",
            ],
        )
        .await
        .unwrap_or_default();
        for line in list.lines() {
            let f = split(line);
            let [inuse, ssid, signal, security] = &f[..] else {
                continue;
            };
            if ssid.is_empty() || networks.iter().any(|n| &n.ssid == ssid) {
                continue;
            }
            networks.push(Network {
                active: inuse == "*",
                known: known.contains(ssid),
                signal: signal.parse().unwrap_or(0),
                secure: !security.is_empty() && security != "--",
                ssid: ssid.clone(),
            });
        }
        networks.sort_by(|a, b| b.active.cmp(&a.active).then(b.signal.cmp(&a.signal)));
        networks.truncate(15);
    }
    // The internet goes through Wi-Fi alone if no other connected device carries it.
    let devs = run("nmcli", &["-t", "-f", "TYPE,STATE", "dev"])
        .await
        .unwrap_or_default();
    let other = devs.lines().any(|l| {
        let (t, s) = l.split_once(':').unwrap_or(("", ""));
        s == "connected" && !matches!(t, "wifi" | "loopback" | "wifi-p2p")
    });
    let connected = networks.iter().find(|n| n.active).map(|n| n.ssid.clone());
    Some(Wifi {
        enabled,
        uplink: connected.is_some() && !other,
        connected,
        networks,
    })
}

/// `nmcli -t` separates fields with `:` and escapes a literal one as `\:`.
fn split(line: &str) -> Vec<String> {
    let mut out = vec![String::new()];
    let mut esc = false;
    for c in line.chars() {
        match (esc, c) {
            (false, '\\') => esc = true,
            (false, ':') => out.push(String::new()),
            _ => {
                esc = false;
                out.last_mut().expect("never empty").push(c);
            }
        }
    }
    out
}

fn unescape(s: &str) -> String {
    split(s).join(":")
}

#[cfg(test)]
mod tests {
    #[test]
    fn colons_inside_fields_survive() {
        assert_eq!(
            super::split(r"*:Cafe\: Guest:72:WPA2"),
            ["*", "Cafe: Guest", "72", "WPA2"]
        );
        assert_eq!(super::split(":Open::"), ["", "Open", "", ""]);
    }
}
