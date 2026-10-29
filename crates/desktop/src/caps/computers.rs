// SPDX-License-Identifier: AGPL-3.0-only
//! Other wado computers, for client mode: the list this app keeps, and whether each is online.
//!
//! The list is this app's own (`$XDG_CONFIG_HOME/wado-desktop/computers.json`) — the web client
//! keeps its list in the browser, and `~/.config/wado` is the daemon's, so neither is touched.
//! Online means the computer's relay answers `/online/<id>` with daemons > 0: the same question
//! the web client's home asks, through `wado::cli::status::https_get`.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Computer {
    pub name: String,
    /// Nine digits.
    pub id: String,
    /// `https://…` (or `http://` for a rig's own relay).
    pub relay: String,
}

/// What the relay said.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Seen {
    Checking,
    Online(u32),
    Offline,
    /// The relay itself did not answer — not the same as the computer being off.
    NoRelay,
}

fn file() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_default()
                .join(".config")
        });
    base.join("wado-desktop/computers.json")
}

pub fn load() -> Vec<Computer> {
    std::fs::read(file())
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

pub fn save(list: &[Computer]) -> Result<(), String> {
    let path = file();
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(
        &tmp,
        serde_json::to_vec_pretty(list).map_err(|e| e.to_string())?,
    )
    .and_then(|_| std::fs::rename(&tmp, &path))
    .map_err(|e| format!("{}: {e}", path.display()))
}

/// The public relay, as a device reaches it.
pub fn default_relay() -> String {
    wado_config::schema::server::DEFAULT_RELAY.replacen("wss://", "https://", 1)
}

/// This computer, if it has a Remote ID — so the list is never empty on a host.
pub fn this_computer() -> Option<Computer> {
    let id = std::fs::read_to_string(wado_config::paths::config_dir().join("remote_id")).ok()?;
    let id: String = id.chars().filter(char::is_ascii_digit).collect();
    (id.len() == 9).then(|| Computer {
        name: super::daemon::hostname(),
        relay: wado_config::live::current()
            .server
            .public_url()
            .unwrap_or_else(default_relay),
        id,
    })
}

/// Read what a person typed or pasted: a Remote ID (`872-990-894`), or a connect link
/// (`…?relay=https://r&id=872990894[&pair=…]`), whose relay wins over the field.
pub fn parse(name: &str, id_or_link: &str, relay: &str) -> Result<Computer, String> {
    let s = id_or_link.trim();
    let (id, link_relay) = if s.contains("id=") {
        let q = s.split_once('?').map_or(s, |(_, q)| q);
        let get = |k: &str| {
            q.split('&')
                .find_map(|kv| kv.strip_prefix(&format!("{k}=")))
                .map(str::to_string)
        };
        (get("id").unwrap_or_default(), get("relay"))
    } else {
        (s.to_string(), None)
    };
    let id: String = id.chars().filter(char::is_ascii_digit).collect();
    if id.len() != 9 {
        return Err("A Remote ID is nine digits, like 123-456-789.".into());
    }
    let relay = link_relay
        .or_else(|| Some(relay.trim().to_string()).filter(|r| !r.is_empty()))
        .unwrap_or_else(default_relay);
    if !(relay.starts_with("https://") || relay.starts_with("http://")) {
        return Err("A relay is an https:// address.".into());
    }
    let name = name.trim();
    Ok(Computer {
        name: if name.is_empty() {
            format!("Computer {}", pretty(&id))
        } else {
            name.to_string()
        },
        id,
        relay: relay.trim_end_matches('/').to_string(),
    })
}

/// Ask the computer's relay whether it is up. Blocking, a few seconds at worst: off-thread.
pub fn probe(c: &Computer) -> Seen {
    let (tls, rest) = match c.relay.split_once("://") {
        Some(("https", r)) => (true, r),
        Some((_, r)) => (false, r),
        None => (true, c.relay.as_str()),
    };
    let host = rest.split('/').next().unwrap_or(rest);
    let path = format!("/online/{}", c.id);
    let body = if tls {
        wado::cli::status::https_get(host, &path)
    } else {
        wado::cli::status::get(host, &path).map(|(_, b)| b)
    };
    let Some(body) = body else {
        return Seen::NoRelay;
    };
    match serde_json::from_str::<serde_json::Value>(&body)
        .ok()
        .and_then(|v| v.get("daemons")?.as_u64())
    {
        Some(0) => Seen::Offline,
        Some(n) => Seen::Online(n as u32),
        None => Seen::NoRelay,
    }
}

/// `872990894` → `872-990-894`.
pub fn pretty(id: &str) -> String {
    wado::cli::status::pretty_id(id)
}

/// The web client's connect link for `c` (`?relay=…&id=…`, what `wado::cli::qr::link` builds).
/// It opens that computer; the web client takes no deeper door than that.
pub fn web_link(c: &Computer) -> String {
    let client = wado_config::live::current().server.client_url.clone();
    wado::cli::qr::link(&client, &c.relay, &c.id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_id_or_a_link_is_read() {
        let c = parse("Studio", "872-990-894", "").unwrap();
        assert_eq!(c.id, "872990894");
        assert_eq!(c.relay, default_relay());
        let l = parse(
            "",
            "https://sandptel.github.io/wado/?relay=https://r.example&id=872990894&pair=abc",
            "",
        )
        .unwrap();
        assert_eq!(l.relay, "https://r.example");
        assert_eq!(l.name, "Computer 872-990-894");
        assert!(parse("x", "1234", "").is_err());
        assert!(parse("x", "872990894", "ftp://nope").is_err());
    }
}
