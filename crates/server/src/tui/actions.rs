//! What the panel changes, and where: the trust list through [`Gate`], settings through `ui.kdl`
//! (exactly what a phone's settings write), then a nudge so the daemons reload now rather than
//! on their next poll. Every function returns the line to show in the footer.

use std::{io::Write, os::unix::net::UnixStream};

use crate::gate::{Gate, Grants};

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

/// Shut a device's passkey unlock now, on every running daemon: it asks for its passkey again.
pub fn passkey_lock(key: &str, name: &str) -> Outcome {
    crate::cli::passkey::lock_everywhere(Some(key));
    Ok(format!("locked {name} — it asks for its passkey again"))
}

/// Ask a device for its passkey again, or stop asking it (`wado passkey require`).
pub fn passkey_require(key: &str, name: &str, on: bool) -> Outcome {
    crate::passkey::Store::default()
        .set_required(key, on)
        .map_err(|e| e.to_string())?;
    if on {
        crate::cli::passkey::lock_everywhere(Some(key));
        Ok(format!("{name} is asked for its passkey again"))
    } else {
        Ok(format!(
            "{name} is no longer asked for a passkey — P turns it back on"
        ))
    }
}

/// Answer a phone that asked to be let in without its passkey.
pub fn approve(id: &str, device: &str, yes: bool) -> Outcome {
    if crate::passkey::approve::Approvals::default().answer(id, yes) {
        Ok(if yes {
            format!("{device} is let in — as with its passkey")
        } else {
            format!("{device} was turned away")
        })
    } else {
        Err(format!("{device} is no longer waiting"))
    }
}

/// A fresh pairing code carrying `grants`, and its link.
pub fn pair(grants: &Grants) -> Result<crate::cli::qr::Pairing, String> {
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
