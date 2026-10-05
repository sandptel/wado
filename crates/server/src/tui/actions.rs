//! What the panel changes, and where: the trust list through [`Gate`], settings through `ui.kdl`
//! (exactly what a phone's settings write), then a nudge so the daemons reload now rather than
//! on their next poll. Every function returns the line to show in the footer.

use std::{io::Write, os::unix::net::UnixStream};

use crate::gate::{Gate, Grants, Verdict};

pub type Outcome = Result<String, String>;

pub fn toggle(key: &str, token: &str, on: bool) -> Outcome {
    Gate::default()
        .allow(key, &[token], on)
        .map(|(name, g)| format!("{name} may now: {}", crate::cli::devices::describe(&g)))
}

pub fn unpair(key: &str) -> Outcome {
    Gate::default()
        .unpair(key)
        .map(|name| format!("unpaired {name}"))
}

pub fn answer(id: &str, name: &str, v: Verdict) -> Outcome {
    if !Gate::default().answer(id, v) {
        return Err(format!("{name} stopped waiting"));
    }
    Ok(match v {
        Verdict::Once => format!("let {name} in once"),
        Verdict::Always => format!("trusted {name}"),
        Verdict::Deny => format!("turned {name} away"),
    })
}

/// A fresh pairing link carrying `grants`.
pub fn pair(grants: &Grants) -> Result<String, String> {
    crate::cli::qr::pair_link(None, None, grants)
}

/// Write a setting to `ui.kdl` and have every running daemon reload it.
pub fn set(key: &str, value: &str) -> Outcome {
    wado_config::ui_file::set(key, wado_config::ui_file::parse_value(value))
        .map_err(|e| e.to_string())?;
    for inst in &crate::cli::status::read().daemons {
        if let Ok(mut s) = UnixStream::connect(crate::config::socket::path_for(inst)) {
            let _ = s.write_all(b"reload\n");
        }
    }
    Ok(format!("{key} = {value}"))
}

/// Put `text` on the clipboard of the terminal the panel is in — OSC 52, so it works over ssh.
pub fn copy(text: &str) {
    use base64::Engine;
    let b64 = base64::engine::general_purpose::STANDARD.encode(text);
    let mut out = std::io::stdout();
    let _ = write!(out, "\x1b]52;c;{b64}\x07");
    let _ = out.flush();
}
