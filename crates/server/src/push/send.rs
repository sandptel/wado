//! One push, delivered: an HTTPS `POST` to the subscription's endpoint (RFC 8030), by hand over
//! rustls like the relay probe in `cli::status` — no HTTP client crate for one request shape.
//!
//! The endpoint comes from a device, so the daemon will not be pointed at itself: addresses on
//! this machine (loopback), link-local ones (cloud metadata lives there) and the like are
//! refused after the name is resolved. Private LAN addresses stay allowed, for a self-hosted
//! UnifiedPush server.

use std::io::{Read, Write};
use std::net::{IpAddr, SocketAddr, TcpStream, ToSocketAddrs};
use std::sync::Arc;
use std::time::Duration;

const TIMEOUT: Duration = Duration::from_secs(10);

/// May the daemon connect here on a device's say-so?
pub fn allowed(ip: IpAddr) -> bool {
    let ip = match ip {
        IpAddr::V6(v6) => v6.to_ipv4_mapped().map_or(IpAddr::V6(v6), IpAddr::V4),
        v4 => v4,
    };
    match ip {
        IpAddr::V4(v) => {
            !(v.is_loopback()
                || v.is_link_local()
                || v.is_unspecified()
                || v.is_multicast()
                || v.is_broadcast()
                || v.octets()[0] == 0)
        }
        IpAddr::V6(v) => {
            let seg = v.segments()[0];
            !(v.is_loopback() || v.is_unspecified() || v.is_multicast() || (seg & 0xffc0) == 0xfe80)
        }
    }
}

/// `(host, port, path)` of an `https://` URL.
fn split(url: &str) -> Result<(String, u16, String), String> {
    let rest = url
        .strip_prefix("https://")
        .ok_or("a push endpoint must be https")?;
    let (authority, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    let (host, port) = if let Some(v6) = authority.strip_prefix('[') {
        let (h, after) = v6.split_once(']').ok_or("bad push endpoint")?;
        (h.to_string(), after.strip_prefix(':').unwrap_or("443"))
    } else {
        match authority.rsplit_once(':') {
            Some((h, p)) => (h.to_string(), p),
            None => (authority.to_string(), "443"),
        }
    };
    let port = port.parse().map_err(|_| "bad port in push endpoint")?;
    if host.is_empty() || host.contains('@') || path.contains(['\r', '\n', ' ']) {
        return Err("bad push endpoint".into());
    }
    Ok((host, port, path.to_string()))
}

/// POST `body` with `headers` to `url`; the HTTP status.
pub fn post(url: &str, headers: &[(&str, String)], body: &[u8]) -> Result<u16, String> {
    let (host, port, path) = split(url)?;
    let addr: SocketAddr = (host.as_str(), port)
        .to_socket_addrs()
        .map_err(|e| format!("could not look up {host}: {e}"))?
        .find(|a| allowed(a.ip()))
        .ok_or_else(|| format!("{host} is not a place this computer sends notifications"))?;
    let roots = rustls::RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let cfg = rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .map_err(|e| e.to_string())?
    .with_root_certificates(roots)
    .with_no_client_auth();
    let name = rustls::pki_types::ServerName::try_from(host.clone()).map_err(|e| e.to_string())?;
    let conn = rustls::ClientConnection::new(Arc::new(cfg), name).map_err(|e| e.to_string())?;
    let tcp = TcpStream::connect_timeout(&addr, TIMEOUT).map_err(|e| e.to_string())?;
    tcp.set_read_timeout(Some(TIMEOUT)).ok();
    tcp.set_write_timeout(Some(TIMEOUT)).ok();
    let mut tls = rustls::StreamOwned::new(conn, tcp);
    let mut req = format!(
        "POST {path} HTTP/1.1\r\nHost: {host}\r\nContent-Length: {}\r\nConnection: close\r\n",
        body.len()
    );
    for (k, v) in headers {
        if v.contains(['\r', '\n']) {
            return Err("bad header".into());
        }
        req.push_str(&format!("{k}: {v}\r\n"));
    }
    req.push_str("\r\n");
    tls.write_all(req.as_bytes()).map_err(|e| e.to_string())?;
    tls.write_all(body).map_err(|e| e.to_string())?;
    let mut head = [0u8; 64];
    let mut got = 0;
    // The status line is all that is read: the body says nothing a retry would act on.
    while got < 12 {
        match tls.read(&mut head[got..]) {
            Ok(0) => break,
            Ok(n) => got += n,
            Err(e) => return Err(e.to_string()),
        }
    }
    let line = String::from_utf8_lossy(&head[..got]);
    line.split(' ')
        .nth(1)
        .and_then(|c| c.get(..3))
        .and_then(|c| c.parse().ok())
        .ok_or_else(|| "the push service gave no answer".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn never_pointed_at_this_machine() {
        for bad in [
            "127.0.0.1",
            "169.254.169.254",
            "0.0.0.0",
            "::1",
            "fe80::1",
            "::ffff:127.0.0.1",
            "224.0.0.1",
        ] {
            assert!(!allowed(bad.parse().unwrap()), "{bad}");
        }
        for ok in ["192.168.1.20", "10.0.0.5", "142.250.1.1", "2606:4700::1"] {
            assert!(allowed(ok.parse().unwrap()), "{ok}");
        }
    }

    #[test]
    fn endpoints_split() {
        assert_eq!(
            split("https://fcm.googleapis.com/fcm/send/a:b").unwrap(),
            ("fcm.googleapis.com".into(), 443, "/fcm/send/a:b".into())
        );
        assert_eq!(
            split("https://ntfy.lan:8443/up").unwrap(),
            ("ntfy.lan".into(), 8443, "/up".into())
        );
        assert_eq!(split("https://[2001:db8::1]/x").unwrap().0, "2001:db8::1");
        assert!(split("http://x/y").is_err());
        assert!(split("https://x/y\r\nEvil: 1").is_err());
        assert!(
            post("https://127.0.0.1/x", &[], b"").is_err(),
            "refused before connecting"
        );
    }
}
