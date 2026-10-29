// SPDX-License-Identifier: AGPL-3.0-only
//! A made-up computer for `--screenshot --fixture`: every directory the daemon's library reads
//! (`XDG_CONFIG_HOME`, `XDG_RUNTIME_DIR`, `XDG_CACHE_HOME`, `TMPDIR`) is pointed into a scratch
//! folder and filled with a plausible pool — three devices, a running session, a log, two
//! daemon sockets and a stand-in relay on loopback. Nothing of the real computer's trust list,
//! config or daemons is read or written, and every name and number in it is invented.
//!
//! Test and demo tooling only: `--fixture` (with or without `--screenshot`) is the only caller.

use std::{
    io::{Read, Write},
    net::TcpListener,
    os::unix::net::UnixListener,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

/// Kept alive for as long as the screenshots run: the sockets answer, the relay serves.
pub struct Fixture {
    _sockets: Vec<UnixListener>,
}

/// The made-up Remote IDs, and how many daemons the stand-in relay says each has.
const HERE: &str = "418203775";
const STUDIO: (&str, u32) = ("507912334", 2);
const DEN: (&str, u32) = ("661045288", 0);

pub fn install(root: &Path) -> std::io::Result<Fixture> {
    let _ = std::fs::remove_dir_all(root);
    // A socket path is at most 108 bytes, so the runtime directory lives somewhere short.
    let run = std::env::temp_dir().join(format!("wdfx-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&run);
    let dirs = [
        root.join("config"),
        run,
        root.join("cache"),
        root.join("tmp"),
    ];
    for d in &dirs {
        std::fs::create_dir_all(d)?;
    }
    // SAFETY: called from `main` before the UI or any producer thread exists.
    unsafe {
        // The window still has to find the real compositor: a relative WAYLAND_DISPLAY is
        // resolved against XDG_RUNTIME_DIR, which is about to move, so pin it absolute.
        if let (Some(rt), Some(wd)) = (
            std::env::var_os("XDG_RUNTIME_DIR"),
            std::env::var_os("WAYLAND_DISPLAY"),
        ) {
            if !Path::new(&wd).is_absolute() {
                std::env::set_var("WAYLAND_DISPLAY", Path::new(&rt).join(wd));
            }
        }
        std::env::set_var("XDG_CONFIG_HOME", &dirs[0]);
        std::env::set_var("XDG_RUNTIME_DIR", &dirs[1]);
        std::env::set_var("XDG_CACHE_HOME", &dirs[2]);
        std::env::set_var("TMPDIR", &dirs[3]);
        std::env::remove_var("WADO_RELAY_URL");
        std::env::remove_var("WADO_PUBLIC_RELAY");
    }
    let port = relay()?;
    let wado = dirs[0].join("wado");
    std::fs::create_dir_all(wado.join("sessions"))?;
    std::fs::create_dir_all(wado.join("links"))?;
    std::fs::write(
        wado.join("config.kdl"),
        format!(
            "{}\nserver {{\n    relay \"ws://127.0.0.1:{port}\"\n    public-relay \"http://127.0.0.1:{port}\"\n}}\n",
            strip_server(wado_config::DEFAULT_KDL)
        ),
    )?;
    std::fs::write(
        wado.join("ui.kdl"),
        "// Written by wado from the client.\nstream {\n    encoder \"hardware\"\n}\ntrackpad {\n    rate-hz 144\n}\n",
    )?;
    std::fs::write(wado.join("remote_id"), HERE)?;
    std::fs::write(
        wado.join("trusted_clients"),
        "c1a9f0e2b7d34e58a1f3\tPixel 8 · Chrome\tpk-a\t1\trw\tshells,host,clipboard\n\
         7be03d9a54c1f2e8b6a0\tiPad · Safari\tpk-b\t1\tro\thost\n\
         f3d2c1b0a9e8d7c6b5a4\tLinux · Firefox\tpk-c\t0\tnone\t-\n",
    )?;
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64);
    std::fs::write(
        wado.join("sessions/418203775_1.json"),
        serde_json::json!({
            "instance": format!("{HERE}:1"),
            "started_ms": now_ms - 3_725_000,
            "width": 1080, "height": 2400, "fps": 90,
            "apps": ["firefox", "foot", "nautilus"],
            "viewer": "Pixel 8 · Chrome",
            "detached": false,
        })
        .to_string(),
    )?;
    std::fs::write(
        wado.join("sessions/418203775_2.json"),
        serde_json::json!({
            "instance": format!("{HERE}:2"),
            "started_ms": now_ms - 26_000_000,
            "width": 2048, "height": 1536, "fps": 60,
            "apps": ["krita"],
            "viewer": null,
            "detached": true,
        })
        .to_string(),
    )?;
    std::fs::write(
        wado.join("links/418203775_1.json"),
        serde_json::json!({
            "key": "c1a9f0e2b7d34e58a1f3",
            "name": "Pixel 8 · Chrome",
            "link": { "state": "direct", "path": "internet", "rtt_ms": 23, "net": "cellular",
                      "visible": true, "last_recovery_ms": null, "last_recovery_why": "" },
            "at_ms": now_ms,
        })
        .to_string(),
    )?;
    std::fs::create_dir_all(dirs[0].join("wado-desktop"))?;
    std::fs::write(
        dirs[0].join("wado-desktop/computers.json"),
        serde_json::json!([
            { "name": "Studio", "id": STUDIO.0, "relay": format!("http://127.0.0.1:{port}") },
            { "name": "Den", "id": DEN.0, "relay": format!("http://127.0.0.1:{port}") },
        ])
        .to_string(),
    )?;
    std::fs::create_dir_all(dirs[2].join("wado"))?;
    std::fs::write(dirs[2].join("wado/daemon.log"), log())?;
    let sockets = ["1", "2"]
        .iter()
        .filter_map(|i| UnixListener::bind(dirs[1].join(format!("wado-{i}.sock"))).ok())
        .collect();
    Ok(Fixture { _sockets: sockets })
}

/// Put a typed pairing's check words to "the computer", as a daemon would.
pub fn ask(device: &str, words: &str) {
    let typed = wado::gate::typed_code::new();
    wado::gate::Gate::default().ask(&typed, words, device);
}

fn strip_server(kdl: &str) -> String {
    // The fixture writes its own `server { }`; drop the commented starting one.
    let mut out = String::new();
    let mut skip = false;
    for l in kdl.lines() {
        if l.trim_start().starts_with("server {") {
            skip = true;
        }
        if !skip {
            out.push_str(l);
            out.push('\n');
        }
        if skip && l.trim() == "}" {
            skip = false;
        }
    }
    out
}

/// A stand-in relay on loopback: `/health` for the pool, `/online/<id>` for the computers.
fn relay() -> std::io::Result<u16> {
    let l = TcpListener::bind("127.0.0.1:0")?;
    let port = l.local_addr()?.port();
    std::thread::spawn(move || {
        for s in l.incoming().flatten() {
            // One thread per connection: a slow one never holds up the next.
            std::thread::spawn(move || serve(s));
        }
    });
    Ok(port)
}

fn serve(mut s: std::net::TcpStream) {
    let _ = s.set_read_timeout(Some(std::time::Duration::from_secs(2)));
    let mut req = Vec::new();
    let mut buf = [0u8; 1024];
    while !req.windows(4).any(|w| w == b"\r\n\r\n") {
        match s.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => req.extend_from_slice(&buf[..n]),
        }
    }
    let req = String::from_utf8_lossy(&req);
    let path = req.split_whitespace().nth(1).unwrap_or("/");
    let body = if path == "/health" {
        r#"{"status":"ok","servers":2,"rooms":1}"#.to_string()
    } else if let Some(id) = path.strip_prefix("/online/") {
        let n = [STUDIO, DEN].iter().find(|c| c.0 == id).map_or(2, |c| c.1);
        format!(r#"{{"daemons":{n}}}"#)
    } else {
        "{}".into()
    };
    let _ = write!(
        s,
        "HTTP/1.0 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    );
}

fn log() -> String {
    let lines = [
        ("INFO", "wado", "wado 0.0.3 — daemon 1 starting"),
        (
            "INFO",
            "wado::nat",
            "NAT: one mapping for every destination — srflx candidates are usable",
        ),
        (
            "INFO",
            "wado::relay_client",
            "relay client: registered — Remote ID 418-203-775 as instance 1",
        ),
        (
            "INFO",
            "wado::relay_client",
            "relay client: ready — clients can connect with the Remote ID",
        ),
        ("DEBUG", "wado::relay_client", "relay: ping 41 ms"),
        (
            "INFO",
            "wado::gate",
            "gate: let Pixel 8 · Chrome in (QR-pinned key)",
        ),
        (
            "INFO",
            "wado::relay_client",
            "browser: session ready — encoder hardware",
        ),
        (
            "INFO",
            "wado_compositor::headless",
            "session started 1080x2400@90 9000kbps pipeline=vaapi-dmabuf",
        ),
        (
            "DEBUG",
            "wado::website::pump",
            "pump: 90.0 fps, 0 dropped, encode p50 2.1 ms",
        ),
        (
            "INFO",
            "wado::files",
            "files: Pixel 8 · Chrome downloaded ~/Pictures/trip/IMG_2041.jpg",
        ),
        (
            "WARN",
            "wado::relay_client",
            "browser: link changed — relayed (symmetric NAT on the device's network)",
        ),
        (
            "INFO",
            "wado::relay_client",
            "browser: link changed — direct · internet · 23 ms · cellular",
        ),
        (
            "INFO",
            "wado::shells",
            "shells: Pixel 8 · Chrome opened shell 2 (fish)",
        ),
        (
            "INFO",
            "wado::config::watch",
            "config changed — stream.max-fps 120, applied",
        ),
        (
            "DEBUG",
            "wado::website::pump",
            "pump: 90.0 fps, 0 dropped, encode p50 2.0 ms",
        ),
        (
            "INFO",
            "wado::host",
            "host action: volume 40% from Pixel 8 · Chrome",
        ),
        (
            "WARN",
            "wado::gate",
            "gate: a wrong pairing code was typed (1 of 5)",
        ),
        (
            "INFO",
            "wado::gate",
            "gate: iPad · Safari asked to join daemon 2",
        ),
        (
            "ERROR",
            "wado::clip",
            "clip: wl-paste is not available here (No such file or directory)",
        ),
        (
            "INFO",
            "wado::relay_client",
            "browser: session ready — encoder hardware",
        ),
    ];
    let mut out = String::new();
    for (i, (lvl, target, msg)) in lines.iter().cycle().take(140).enumerate() {
        let s = 9 * 3600 + 12 * 60 + i as u64 * 17;
        out.push_str(&format!(
            "2026-10-30T{:02}:{:02}:{:02}.{:06}Z {lvl:>5} {target}: {msg}\n",
            s / 3600,
            (s / 60) % 60,
            s % 60,
            (i * 7919) % 1_000_000
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_stand_in_relay_answers() {
        let port = super::relay().unwrap();
        let r = wado::cli::status::get(&format!("127.0.0.1:{port}"), "/health");
        assert!(
            r.as_ref()
                .is_some_and(|(c, b)| *c == 200 && b.contains("servers")),
            "{r:?}"
        );
    }
}
