//! Wi-Fi, through NetworkManager's `nmcli`.
//!
//! The dangerous one: on a computer that reaches the internet by Wi-Fi alone, turning it off or
//! joining the wrong network cuts the very connection the viewer is using, and nobody can turn
//! it back from the phone. So every change is put on probation, the way a monitor change asks
//! "keep these settings?": joining a network is undone after [`PROBATION`] unless a viewer
//! confirms it, and turning Wi-Fi off can be for a few minutes only.

use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

use std::sync::Mutex;

use wado_protocol::host::{Network, Wifi};

use super::cmd::run;

pub const PROBATION: Duration = Duration::from_secs(60);

/// Bumped by every change and by Keep; a pending revert only acts if it is still current.
static GEN: AtomicU64 = AtomicU64::new(0);
static DEADLINE: Mutex<Option<Instant>> = Mutex::new(None);

pub async fn state() -> Option<Wifi> {
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

/// Seconds until a change on probation is undone.
pub fn revert_in() -> Option<u32> {
    let d = *DEADLINE.lock().unwrap_or_else(|e| e.into_inner());
    d.map(|t| t.saturating_duration_since(Instant::now()).as_secs() as u32)
}

pub async fn radio(on: bool, minutes: Option<u32>) -> Result<(), String> {
    let g = GEN.fetch_add(1, Ordering::SeqCst) + 1;
    run("nmcli", &["radio", "wifi", if on { "on" } else { "off" }]).await?;
    if !on {
        if let Some(m) = minutes.filter(|m| *m > 0) {
            let back = Duration::from_secs(u64::from(m.min(240)) * 60);
            *DEADLINE.lock().unwrap_or_else(|e| e.into_inner()) = Some(Instant::now() + back);
            tokio::spawn(async move {
                tokio::time::sleep(back).await;
                if GEN.load(Ordering::SeqCst) == g {
                    tracing::info!("turning Wi-Fi back on, as asked");
                    let _ = run("nmcli", &["radio", "wifi", "on"]).await;
                    *DEADLINE.lock().unwrap_or_else(|e| e.into_inner()) = None;
                }
            });
        }
    } else {
        *DEADLINE.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }
    Ok(())
}

/// Join `ssid`, on probation: if nobody confirms within [`PROBATION`], go back to the network
/// this computer was on.
pub async fn connect(ssid: &str, password: Option<&str>) -> Result<(), String> {
    let previous = state().await.and_then(|w| w.connected);
    let g = GEN.fetch_add(1, Ordering::SeqCst) + 1;
    let joined = match password.filter(|p| !p.is_empty()) {
        Some(pw) => run("nmcli", &["dev", "wifi", "connect", ssid, "password", pw]).await,
        None => match run("nmcli", &["connection", "up", "id", ssid]).await {
            Ok(o) => Ok(o),
            Err(_) => run("nmcli", &["dev", "wifi", "connect", ssid]).await,
        },
    };
    if let (Some(prev), true) = (previous.clone(), previous.as_deref() != Some(ssid)) {
        *DEADLINE.lock().unwrap_or_else(|e| e.into_inner()) = Some(Instant::now() + PROBATION);
        tokio::spawn(async move {
            tokio::time::sleep(PROBATION).await;
            if GEN.load(Ordering::SeqCst) == g {
                tracing::warn!(network = prev, "Wi-Fi change not confirmed — going back");
                let _ = run("nmcli", &["connection", "up", "id", &prev]).await;
                *DEADLINE.lock().unwrap_or_else(|e| e.into_inner()) = None;
            }
        });
    }
    joined.map(drop)
}

/// A viewer is back and says the change is good.
pub fn keep() {
    GEN.fetch_add(1, Ordering::SeqCst);
    *DEADLINE.lock().unwrap_or_else(|e| e.into_inner()) = None;
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
