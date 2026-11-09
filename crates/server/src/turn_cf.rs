//! Cloudflare's TURN: credentials minted from the configured key (`server { turn {
//! cloudflare-key …; cloudflare-token … } }`), good for a day, renewed at half-life, and handed
//! to each device with [`crate::ice::for_devices`]. A TURN relay is the one path that connects
//! two ends that are both behind a VPN or a symmetric NAT (see [`crate::ice`]).

use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde_json::Value;
use wado_protocol::relay::IceServer;

/// How long minted credentials last, in seconds.
pub const TTL_SECS: u64 = 86_400;
const API: &str = "https://rtc.live.cloudflare.com/v1/turn/keys";
/// How often the key is looked at again: a config change takes effect within this.
const CHECK: Duration = Duration::from_secs(60);

struct Minted {
    servers: Vec<IceServer>,
    until: Instant,
}

static NOW: Mutex<Option<Minted>> = Mutex::new(None);

/// The servers in hand and the seconds they have left; `None` when unconfigured or not yet
/// minted.
pub fn current() -> Option<(Vec<IceServer>, u64)> {
    let now = NOW.lock().unwrap_or_else(|e| e.into_inner());
    let m = now.as_ref()?;
    let left = m.until.checked_duration_since(Instant::now())?.as_secs();
    Some((m.servers.clone(), left))
}

/// The key and token, when both are set.
fn configured() -> Option<(String, String)> {
    let t = wado_config::live::current().server.turn.clone()?;
    (!t.cloudflare_key.is_empty() && !t.cloudflare_token.is_empty())
        .then_some((t.cloudflare_key, t.cloudflare_token))
}

pub fn is_configured() -> bool {
    configured().is_some()
}

/// Ask Cloudflare for a day's credentials. Blocking: one HTTPS request.
fn mint(key: &str, token: &str) -> Result<Vec<IceServer>, String> {
    if key.is_empty()
        || !key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err("the TURN key ID is not a Cloudflare key ID".into());
    }
    let (status, body) = crate::push::send::post_reading(
        &format!("{API}/{key}/credentials/generate-ice-servers"),
        &[
            ("Authorization", format!("Bearer {token}")),
            ("Content-Type", "application/json".into()),
        ],
        format!("{{\"ttl\":{TTL_SECS}}}").as_bytes(),
    )?;
    if !(200..300).contains(&status) {
        let why: String = body.chars().take(200).collect();
        return Err(format!("Cloudflare answered {status}: {why}"));
    }
    parse(&body)
}

/// The servers in Cloudflare's answer — `{"iceServers": [...]}`, or the older single object —
/// without the port-53 URLs, which browsers refuse and some networks drop.
pub fn parse(body: &str) -> Result<Vec<IceServer>, String> {
    let v: Value = serde_json::from_str(body).map_err(|e| format!("an unreadable answer: {e}"))?;
    let list = match &v["iceServers"] {
        Value::Array(a) => a.as_slice(),
        o @ Value::Object(_) => std::slice::from_ref(o),
        _ => return Err("no iceServers in the answer".into()),
    };
    let out: Vec<IceServer> = list
        .iter()
        .filter_map(|s| {
            let urls: Vec<String> = match &s["urls"] {
                u @ Value::String(_) => std::slice::from_ref(u),
                Value::Array(a) => a.as_slice(),
                _ => &[],
            }
            .iter()
            .filter_map(Value::as_str)
            .filter(|u| !on_port_53(u))
            .map(String::from)
            .collect();
            (!urls.is_empty()).then(|| IceServer {
                urls,
                username: s["username"].as_str().unwrap_or("").to_string(),
                credential: s["credential"].as_str().unwrap_or("").to_string(),
            })
        })
        .collect();
    if out
        .iter()
        .any(|s| s.urls.iter().any(|u| u.starts_with("turn")))
    {
        Ok(out)
    } else {
        Err("the answer had no TURN server".into())
    }
}

fn on_port_53(url: &str) -> bool {
    url.split('?').next().is_some_and(|u| u.ends_with(":53"))
}

/// Keep credentials fresh while a key is configured; forget them when it is removed.
pub async fn keep() {
    loop {
        match configured() {
            None => *NOW.lock().unwrap_or_else(|e| e.into_inner()) = None,
            Some((key, token)) => {
                let stale = current().is_none_or(|(_, left)| left < TTL_SECS / 2);
                if stale {
                    match tokio::task::spawn_blocking(move || mint(&key, &token)).await {
                        Ok(Ok(servers)) => {
                            tracing::info!(
                                urls = servers.iter().map(|s| s.urls.len()).sum::<usize>(),
                                "TURN: Cloudflare credentials for the next day"
                            );
                            *NOW.lock().unwrap_or_else(|e| e.into_inner()) = Some(Minted {
                                servers,
                                until: Instant::now() + Duration::from_secs(TTL_SECS),
                            });
                        }
                        Ok(Err(e)) => tracing::warn!("TURN: Cloudflare credentials failed — {e}"),
                        Err(e) => tracing::warn!("TURN: {e}"),
                    }
                }
            }
        }
        tokio::time::sleep(CHECK).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_cloudflares_answer() {
        let body = r#"{"iceServers":[{"urls":["stun:stun.cloudflare.com:3478","stun:stun.cloudflare.com:53"]},
            {"urls":["turn:turn.cloudflare.com:3478?transport=udp","turn:turn.cloudflare.com:53?transport=udp",
            "turns:turn.cloudflare.com:443?transport=tcp"],"username":"u1","credential":"c1"}]}"#;
        let s = parse(body).unwrap();
        assert_eq!(s.len(), 2);
        assert_eq!(s[0].urls, ["stun:stun.cloudflare.com:3478"]);
        assert_eq!(
            s[1].urls,
            [
                "turn:turn.cloudflare.com:3478?transport=udp",
                "turns:turn.cloudflare.com:443?transport=tcp"
            ]
        );
        assert_eq!(
            (s[1].username.as_str(), s[1].credential.as_str()),
            ("u1", "c1")
        );
        // The older shape: one object.
        let old = r#"{"iceServers":{"urls":["turn:t:3478"],"username":"u","credential":"c"}}"#;
        assert_eq!(parse(old).unwrap()[0].username, "u");
        assert!(
            parse(r#"{"iceServers":[{"urls":"stun:x:3478"}]}"#).is_err(),
            "no TURN in it"
        );
    }

    #[test]
    fn a_strange_key_is_not_sent() {
        assert!(mint("../x", "t").is_err());
    }
}
