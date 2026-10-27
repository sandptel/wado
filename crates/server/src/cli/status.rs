//! What is running on this computer, read from the outside: the daemons' command sockets, the
//! relay's health, and the files the pool shares. Used by the landing page and the TUI.

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
    let remote_id = std::fs::read_to_string(wado_config::paths::config_dir().join("remote_id"))
        .ok()
        .map(|s| pretty_id(s.trim()))
        .filter(|s| !s.is_empty());
    Status {
        daemons: daemons(),
        relay: relay_health(),
        relay_addr: host_of(&relay_url()),
        tunnel: tunnel(),
        remote_id,
        sessions: crate::sessions::list(),
        trusted: gate.trusted().len(),
        pending: gate.pending().len(),
    }
}

impl Status {
    /// How many of this computer's daemons the relay has registered: `daemons` from the public
    /// relay's `/online/<id>`, or `servers` from a private relay's `/health`.
    pub fn pooled(&self) -> Option<u32> {
        let h = self.relay.as_deref()?;
        let n = h
            .split("\"daemons\":")
            .nth(1)
            .or_else(|| h.split("\"servers\":").nth(1))?;
        n.split(|c: char| !c.is_ascii_digit()).next()?.parse().ok()
    }
}

/// Instances with a live socket, sorted. A file a dead daemon left behind does not connect.
pub fn daemons() -> Vec<String> {
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

/// The relay this computer's daemons dial: `WADO_RELAY_URL`, else config — the public relay
/// unless set — else the rig's local one.
fn relay_url() -> String {
    std::env::var("WADO_RELAY_URL")
        .ok()
        .or_else(|| {
            wado_config::live::current()
                .server
                .relay_url()
                .map(str::to_owned)
        })
        .unwrap_or_else(|| "ws://127.0.0.1:4000".into())
}

/// `wss://host:port/path` → `host:port`.
fn host_of(url: &str) -> String {
    url.split("://")
        .nth(1)
        .unwrap_or(url)
        .split('/')
        .next()
        .unwrap_or("")
        .to_string()
}

/// The relay's answer, refreshed every 5 s off the caller's thread: a relay across the internet
/// must not stall the panel's redraw. The first call waits for one answer, so a one-shot
/// `wado` prints the truth rather than "down".
///
/// ponytail: the URL and Remote ID are re-read each round, not watched.
fn relay_health() -> Option<String> {
    use std::sync::{Mutex, Once};
    static LAST: Mutex<Option<String>> = Mutex::new(None);
    static START: Once = Once::new();
    START.call_once(|| {
        *LAST.lock().unwrap() = probe_relay();
        std::thread::spawn(|| {
            loop {
                std::thread::sleep(Duration::from_secs(5));
                let h = probe_relay();
                *LAST.lock().unwrap() = h;
            }
        });
    });
    LAST.lock().unwrap().clone()
}

/// A TLS relay is a shared one, so it is asked about this computer only (`/online/<id>`); a
/// plain-`ws` one is the rig's own and answers `/health`.
fn probe_relay() -> Option<String> {
    let url = relay_url();
    let addr = host_of(&url);
    if !url.starts_with("wss://") {
        return health(&addr);
    }
    let id = std::fs::read_to_string(wado_config::paths::config_dir().join("remote_id"))
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let path = id.map_or("/health".into(), |id| format!("/online/{id}"));
    https_get(&addr, &path)
}

/// One `GET` over TLS, by hand like [`get`]: rustls and the webpki roots are already in the
/// tree for the relay link itself. The body on a 2xx, else `None`.
fn https_get(host: &str, path: &str) -> Option<String> {
    use std::{net::ToSocketAddrs, sync::Arc};
    let (name, port) = host.split_once(':').unwrap_or((host, "443"));
    let roots = rustls::RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let cfg = rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .ok()?
    .with_root_certificates(roots)
    .with_no_client_auth();
    let server = rustls::pki_types::ServerName::try_from(name.to_string()).ok()?;
    let conn = rustls::ClientConnection::new(Arc::new(cfg), server).ok()?;
    let sock = (name, port.parse::<u16>().ok()?)
        .to_socket_addrs()
        .ok()?
        .next()?;
    let tcp = TcpStream::connect_timeout(&sock, Duration::from_secs(3)).ok()?;
    tcp.set_read_timeout(Some(Duration::from_secs(3))).ok()?;
    let mut tls = rustls::StreamOwned::new(conn, tcp);
    write!(tls, "GET {path} HTTP/1.0\r\nHost: {name}\r\n\r\n").ok()?;
    let mut resp = String::new();
    // A server that closes without close_notify ends in an error after the body has arrived.
    let _ = tls.read_to_string(&mut resp);
    let ok = resp.split(' ').nth(1)?.starts_with('2');
    ok.then(|| {
        resp.split("\r\n\r\n")
            .nth(1)
            .unwrap_or("")
            .trim()
            .to_string()
    })
}

/// `GET /health`, the body.
pub fn health(addr: &str) -> Option<String> {
    get(addr, "/health").map(|(_, body)| body)
}

/// One plain-HTTP `GET` to a local service, by hand: status and body, no HTTP client dependency.
pub fn get(addr: &str, path: &str) -> Option<(u16, String)> {
    let sock = addr.parse().ok()?;
    let mut s = TcpStream::connect_timeout(&sock, Duration::from_millis(300)).ok()?;
    s.set_read_timeout(Some(Duration::from_millis(500))).ok()?;
    write!(s, "GET {path} HTTP/1.0\r\nHost: {addr}\r\n\r\n").ok()?;
    let mut resp = String::new();
    s.read_to_string(&mut resp).ok()?;
    let code = resp.split(' ').nth(1)?.parse().ok()?;
    let body = resp
        .split("\r\n\r\n")
        .nth(1)
        .unwrap_or("")
        .trim()
        .to_string();
    Some((code, body))
}

/// Where the rig keeps its logs — `scripts/rig.sh`'s relay and tunnel, and spawned daemons.
pub fn rig_dir() -> PathBuf {
    std::env::var_os("TMPDIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| "/tmp".into())
        .join("wado-rig")
}

pub fn tunnel() -> Option<String> {
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
