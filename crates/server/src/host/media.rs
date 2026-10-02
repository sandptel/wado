//! Media players — Spotify, a browser tab, mpv — through MPRIS on D-Bus: what is playing, and
//! play/pause, next, previous, seek and shuffle.
//!
//! Two buses are searched: the computer's own session bus, and the wado session's private one
//! (an app launched in an isolated session registers there). Each player is tied to its audio
//! stream through the process id, which is what lets its card say where it is playing and move
//! it.

use std::{collections::HashMap, sync::Mutex};

use base64::Engine;
use wado_protocol::host::{MediaOp, Player, Stream};
use zbus::{
    Connection,
    zvariant::{ObjectPath, OwnedValue, Value},
};

const PREFIX: &str = "org.mpris.MediaPlayer2.";
const PATH: &str = "/org/mpris/MediaPlayer2";
const PLAYER: &str = "org.mpris.MediaPlayer2.Player";
/// Local cover art bigger than this is left out rather than sent on every refresh.
const ART_MAX: u64 = 256 * 1024;

/// One connection per bus address (`""` is the computer's own session bus), kept between polls.
static CONNS: Mutex<Vec<(String, Connection)>> = Mutex::new(Vec::new());

async fn connect(address: &str) -> Option<Connection> {
    if let Some(c) = CONNS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .iter()
        .find(|(a, _)| a == address)
    {
        return Some(c.1.clone());
    }
    let c = if address.is_empty() {
        Connection::session().await.ok()?
    } else {
        zbus::connection::Builder::address(address)
            .ok()?
            .build()
            .await
            .ok()?
    };
    let mut conns = CONNS.lock().unwrap_or_else(|e| e.into_inner());
    // A session bus that went away leaves a dead connection behind; keep the list short.
    conns.retain(|(a, _)| {
        a.is_empty() || Some(a.as_str()) == crate::notify::session_bus().as_deref()
    });
    conns.push((address.to_string(), c.clone()));
    Some(c)
}

fn buses() -> Vec<String> {
    let mut b = vec![String::new()];
    b.extend(crate::notify::session_bus());
    b
}

/// Every player on both buses, playing ones first.
pub async fn state(streams: &[Stream]) -> Vec<Player> {
    let mut out = Vec::new();
    for address in buses() {
        let Some(conn) = connect(&address).await else {
            continue;
        };
        let Ok(dbus) = zbus::fdo::DBusProxy::new(&conn).await else {
            continue;
        };
        let Ok(names) = dbus.list_names().await else {
            continue;
        };
        for name in names
            .iter()
            .map(|n| n.as_str())
            .filter(|n| n.starts_with(PREFIX))
        {
            // playerctld is a proxy for whichever player was used last: a duplicate.
            if name.contains("playerctld") {
                continue;
            }
            let pid = match zbus::names::BusName::try_from(name) {
                Ok(b) => dbus.get_connection_unix_process_id(b).await.ok(),
                Err(_) => None,
            };
            if let Some(p) = player(&conn, name, pid, streams).await {
                out.push(p);
            }
        }
    }
    out.sort_by_key(|p| !p.playing);
    out
}

async fn player(
    conn: &Connection,
    name: &str,
    pid: Option<u32>,
    streams: &[Stream],
) -> Option<Player> {
    let root = zbus::Proxy::new(conn, name, PATH, "org.mpris.MediaPlayer2")
        .await
        .ok()?;
    let p = zbus::Proxy::new(conn, name, PATH, PLAYER).await.ok()?;
    let status: String = p.get_property("PlaybackStatus").await.ok()?;
    let meta: HashMap<String, OwnedValue> = p.get_property("Metadata").await.unwrap_or_default();
    let text = |k: &str| {
        meta.get(k)
            .and_then(|v| String::try_from(v.clone()).ok())
            .unwrap_or_default()
    };
    let artist = meta
        .get("xesam:artist")
        .and_then(|v| <Vec<String>>::try_from(v.clone()).ok())
        .map(|a| a.join(", "))
        .unwrap_or_default();
    let length_us = meta
        .get("mpris:length")
        .and_then(|v| {
            i64::try_from(v.clone())
                .ok()
                .or_else(|| u64::try_from(v.clone()).ok().map(|u| u as i64))
        })
        .unwrap_or(0);
    let position_us: i64 = p.get_property("Position").await.unwrap_or(0);
    let identity: String = root
        .get_property("Identity")
        .await
        .unwrap_or_else(|_| name.trim_start_matches(PREFIX).to_string());
    let flag = |b: Result<bool, _>| b.unwrap_or(false);
    // The stream whose process is this player — or, for browsers, a child of it.
    let stream = pid.and_then(|pid| {
        streams
            .iter()
            .find(|s| s.pid == Some(pid))
            .or_else(|| {
                streams
                    .iter()
                    .find(|s| s.pid.is_some_and(|sp| parent_of(sp) == Some(pid)))
            })
            .map(|s| s.id)
    });
    Some(Player {
        bus: name.to_string(),
        app: identity,
        title: text("xesam:title"),
        artist,
        art: art(&text("mpris:artUrl")),
        playing: status == "Playing",
        position_ms: (position_us.max(0) / 1000) as u64,
        length_ms: (length_us.max(0) / 1000) as u64,
        shuffle: p.get_property("Shuffle").await.ok(),
        can_next: flag(p.get_property("CanGoNext").await),
        can_prev: flag(p.get_property("CanGoPrevious").await),
        can_seek: flag(p.get_property("CanSeek").await) && length_us > 0,
        stream,
    })
}

/// The parent pid, from `/proc` — a browser plays audio from a child of the process that owns
/// its MPRIS name.
fn parent_of(pid: u32) -> Option<u32> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let after = stat.rsplit_once(')')?.1;
    after.split_whitespace().nth(1)?.parse().ok()
}

/// An `https:` URL is passed through; a local file becomes a small `data:` URI; anything else
/// is dropped.
fn art(url: &str) -> Option<String> {
    if url.starts_with("https://") {
        return Some(url.to_string());
    }
    let path = url.strip_prefix("file://")?;
    let path = percent_decode(path);
    let meta = std::fs::metadata(&path).ok()?;
    if meta.len() > ART_MAX {
        return None;
    }
    let bytes = std::fs::read(&path).ok()?;
    let mime = if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        "image/png"
    } else {
        "image/jpeg"
    };
    Some(format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub async fn act(bus: &str, op: MediaOp) -> Result<(), String> {
    if !bus.starts_with(PREFIX) {
        return Err("not a media player".into());
    }
    for address in buses() {
        let Some(conn) = connect(&address).await else {
            continue;
        };
        let Ok(p) = zbus::Proxy::new(&conn, bus, PATH, PLAYER).await else {
            continue;
        };
        let done = match op {
            MediaOp::PlayPause => p.call_method("PlayPause", &()).await.map(drop),
            MediaOp::Next => p.call_method("Next", &()).await.map(drop),
            MediaOp::Previous => p.call_method("Previous", &()).await.map(drop),
            MediaOp::Shuffle { on } => p
                .set_property("Shuffle", on)
                .await
                .map_err(zbus::Error::from),
            MediaOp::SeekTo { ms } => {
                let meta: HashMap<String, OwnedValue> =
                    p.get_property("Metadata").await.unwrap_or_default();
                let track = meta.get("mpris:trackid").and_then(|v| {
                    ObjectPath::try_from(Value::from(v.clone()))
                        .ok()
                        .map(|o| o.into_owned())
                });
                match track {
                    Some(t) => p
                        .call_method("SetPosition", &(t, (ms as i64) * 1000))
                        .await
                        .map(drop),
                    None => Err(zbus::Error::Failure("this player cannot seek".into())),
                }
            }
        };
        match done {
            Ok(()) => return Ok(()),
            // Not on this bus: try the next one.
            Err(zbus::Error::MethodError(n, _, _)) if n.as_str().ends_with("ServiceUnknown") => {
                continue;
            }
            Err(e) => return Err(format!("the player refused: {e}")),
        }
    }
    Err("that player is gone".into())
}

#[cfg(test)]
mod tests {
    #[test]
    fn decodes_file_urls() {
        assert_eq!(
            super::percent_decode("/tmp/My%20Cover.png"),
            "/tmp/My Cover.png"
        );
        assert_eq!(super::art("http://insecure.example/a.png"), None);
        assert_eq!(
            super::art("https://i.example/a.png").as_deref(),
            Some("https://i.example/a.png")
        );
    }
}
