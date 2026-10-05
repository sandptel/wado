//! What is running on this computer, read from the outside: the daemons' command sockets, the
//! relay's `/health`, and the files the pool shares. Used by the landing page and the TUI.

use std::{
    io::{Read, Write},
    net::TcpStream,
    os::unix::net::UnixStream,
    path::PathBuf,
    time::Duration,
};

use wado_protocol::SessionSummary;

/// `872990894` → `872-990-894`, the way a person reads it out.
pub fn pretty_id(id: &str) -> String {
    let digits: Vec<char> = id.chars().filter(char::is_ascii_digit).collect();
    if digits.len() != 9 {
        return id.to_string();
    }
    digits
        .chunks(3)
        .map(|c| c.iter().collect::<String>())
        .collect::<Vec<_>>()
        .join("-")
}

pub struct Status {
    /// Instance names of daemons answering on their command socket.
    pub daemons: Vec<String>,
    /// The relay's `host:port`, and its `/health` body if it answered.
    pub relay_addr: String,
    pub relay: Option<String>,
    /// The rig's tunnel URL, if `scripts/rig.sh` started one.
    pub tunnel: Option<String>,
    pub remote_id: Option<String>,
    pub sessions: Vec<SessionSummary>,
    pub trusted: usize,
    pub pending: usize,
}

pub fn read() -> Status {
    let gate = crate::gate::Gate::default();
    let relay_addr = relay_addr();
    Status {
        daemons: daemons(),
        relay: health(&relay_addr),
        relay_addr,
        tunnel: tunnel(),
        remote_id: std::fs::read_to_string(wado_config::paths::config_dir().join("remote_id"))
            .ok()
            .map(|s| pretty_id(s.trim()))
            .filter(|s| !s.is_empty()),
        sessions: crate::sessions::list(),
        trusted: gate.trusted().len(),
        pending: gate.pending().len(),
    }
}

impl Status {
    /// How many daemons the relay has registered, from its `/health`.
    pub fn pooled(&self) -> Option<u32> {
        let h = self.relay.as_deref()?;
        let n = h.split("\"servers\":").nth(1)?;
        n.split(|c: char| !c.is_ascii_digit()).next()?.parse().ok()
    }
}

/// Instances with a live socket, sorted. A file a dead daemon left behind does not connect.
fn daemons() -> Vec<String> {
    let dir = crate::config::socket::path_for("x")
        .parent()
        .map(PathBuf::from)
        .unwrap_or_default();
    let mut out: Vec<String> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            let inst = name
                .strip_prefix("wado-")?
                .strip_suffix(".sock")?
                .to_string();
            UnixStream::connect(e.path()).ok().map(|_| inst)
        })
        .collect();
    out.sort_by_key(|i| (i.len(), i.clone()));
    out
}

/// The relay this computer's daemons dial, as `host:port`: `WADO_RELAY_URL`, else config, else
/// the rig's `127.0.0.1:4000`.
fn relay_addr() -> String {
    let url = std::env::var("WADO_RELAY_URL")
        .ok()
        .or_else(|| wado_config::live::current().server.relay.clone())
        .unwrap_or_default();
    let host = url
        .split("://")
        .nth(1)
        .unwrap_or("")
        .split('/')
        .next()
        .unwrap_or("");
    if host.is_empty() {
        "127.0.0.1:4000".into()
    } else {
        host.into()
    }
}

/// `GET /health` by hand: one request, no HTTP client dependency for it.
fn health(addr: &str) -> Option<String> {
    let sock = addr.parse().ok()?;
    let mut s = TcpStream::connect_timeout(&sock, Duration::from_millis(300)).ok()?;
    s.set_read_timeout(Some(Duration::from_millis(500))).ok()?;
    write!(s, "GET /health HTTP/1.0\r\nHost: {addr}\r\n\r\n").ok()?;
    let mut body = String::new();
    s.read_to_string(&mut body).ok()?;
    body.split("\r\n\r\n").nth(1).map(|b| b.trim().to_string())
}

/// Where `scripts/rig.sh` keeps its logs.
pub fn rig_dir() -> PathBuf {
    std::env::var_os("TMPDIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| "/tmp".into())
        .join("wado-rig")
}

fn tunnel() -> Option<String> {
    // The log names other cloudflare.com pages first; the tunnel is the trycloudflare one.
    let log = std::fs::read_to_string(rig_dir().join("tunnel.log")).ok()?;
    log.match_indices("https://").find_map(|(at, _)| {
        let url: String = log[at..]
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || ":/.-".contains(*c))
            .collect();
        url.ends_with(".trycloudflare.com").then_some(url)
    })
}
