//! `wado msg` — the daemon's local command socket, `$XDG_RUNTIME_DIR/wado-<instance>.sock`.
//!
//! One request line in, a text answer out, connection closed. Local only, and a local user who
//! can reach the socket can already edit `config.kdl`, so no tier check: this is the owner.
//!
//! ponytail: blocking std sockets on one thread. Requests are human-typed and rare.

use std::{
    io::{BufRead, BufReader, Write},
    os::unix::net::{UnixListener, UnixStream},
    path::PathBuf,
};

pub fn path_for(instance: &str) -> PathBuf {
    let dir = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    dir.join(format!("wado-{instance}.sock"))
}

pub fn start() {
    let path = path_for(&wado_config::live::current().server.instance);
    // A socket left behind by a daemon that died; a live one would still be listening.
    if UnixStream::connect(&path).is_err() {
        let _ = std::fs::remove_file(&path);
    }
    let listener = match UnixListener::bind(&path) {
        Ok(l) => l,
        Err(e) => {
            tracing::warn!("`wado msg` socket {} not available: {e}", path.display());
            return;
        }
    };
    let _ = std::thread::Builder::new()
        .name("wado-msg".into())
        .spawn(move || {
            for stream in listener.incoming().flatten() {
                serve(stream);
            }
        });
}

fn serve(mut stream: UnixStream) {
    let mut line = String::new();
    if BufReader::new(&stream).read_line(&mut line).is_err() {
        return;
    }
    let answer = answer(line.trim());
    let _ = stream.write_all(answer.as_bytes());
}

fn answer(req: &str) -> String {
    let mut words = req.splitn(3, ' ');
    match (words.next(), words.next(), words.next()) {
        (Some("reload"), None, None) => match super::watch::reload() {
            Ok(()) => "ok\n".into(),
            Err(e) => format!("error: {e}\n"),
        },
        (Some("get"), key, None) => get(key.unwrap_or("")),
        (Some("set"), Some(key), value) => {
            match wado_config::ui_file::set(
                key,
                wado_config::ui_file::parse_value(value.unwrap_or("")),
            ) {
                Ok(()) => match super::watch::reload() {
                    Ok(()) => "ok (written to ui.kdl, applied)\n".into(),
                    Err(e) => format!("error: {e}\n"),
                },
                Err(e) => format!("error: {e}\n"),
            }
        }
        (Some("devices"), None, None) => devices(),
        _ => "error: commands are reload | get [key] | set <key> [value] | devices\n".into(),
    }
}

fn get(key: &str) -> String {
    let Ok(mut v) = serde_json::to_value(&*wado_config::live::current()) else {
        return "error: config did not serialise\n".into();
    };
    for seg in key.split('.').filter(|s| !s.is_empty()) {
        // Keys are written kebab-case in KDL and snake_case in the structs.
        v = match v.get(seg).or_else(|| v.get(seg.replace('-', "_"))) {
            Some(x) => x.clone(),
            None => return format!("error: no key {key}\n"),
        };
    }
    format!("{}\n", serde_json::to_string_pretty(&v).unwrap_or_default())
}

fn devices() -> String {
    let gate = crate::gate::Gate::default();
    let mut out = String::new();
    for (key, name) in gate.trusted() {
        let owner = if super::link::is_owner(&key, &gate) {
            "  owner"
        } else {
            ""
        };
        out.push_str(&format!("{name}\t{key}{owner}\n"));
    }
    if out.is_empty() {
        out.push_str("no trusted devices yet\n");
    }
    out
}
