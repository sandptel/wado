//! Input on the Mac: the phone's touchpad, keyboard, typed text and controller, posted as
//! CoreGraphics events. Needs the Accessibility grant — without it macOS drops every event
//! silently, so nothing here is offered until it is granted.
//!
//! - `pad.rs` — fingers read as a Mac trackpad; `hotkeys.rs` — the Mac's Mission Control and
//!   Spaces shortcuts; `keys.rs` — keys and text; `gamepad.rs` — the controller as keys;
//!   `keymap.rs` — evdev → macOS key codes; `post.rs` — the calls; `cg.rs` — CoreGraphics.

pub mod cg;
pub mod gamepad;
pub mod hotkeys;
pub mod keymap;
pub mod keys;
pub mod pad;
pub mod post;

use std::sync::atomic::AtomicU64;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::time::{Duration, Instant};

use wado_platform::input::{InputError, InputRes, TouchInjector};
use wado_protocol::pad::Contact;

/// Modifier flags a phone is holding down now, read by pointer events (⌘-click).
pub static HELD: AtomicU64 = AtomicU64::new(0);

/// `units` in pieces of at most `max`, never splitting a surrogate pair.
pub fn text_chunks(units: &[u16], max: usize) -> Vec<&[u16]> {
    let mut out = Vec::new();
    let mut rest = units;
    while !rest.is_empty() {
        let mut n = rest.len().min(max.max(2));
        if n < rest.len() && (0xD800..0xDC00).contains(&rest[n - 1]) {
            n -= 1;
        }
        out.push(&rest[..n]);
        rest = &rest[n..];
    }
    out
}

enum Msg {
    Resize(u16, u16),
    Frame(Vec<Contact>),
}

/// The touchpad: an [`pad::Interpreter`] on its own thread, so a glide keeps going between
/// frames and posting never waits on the input lane.
pub struct MacTouch {
    tx: Sender<Msg>,
}

impl MacTouch {
    pub fn open() -> Result<MacTouch, InputError> {
        let (tx, rx) = channel();
        std::thread::Builder::new()
            .name("mac-touch".into())
            .spawn(move || run(rx))
            .map_err(|e| InputError::Failed(e.to_string()))?;
        Ok(MacTouch { tx })
    }
}

fn run(rx: Receiver<Msg>) {
    let mut out = cg::CgPoster::new();
    let mut i = pad::Interpreter::new(Instant::now());
    i.hotkeys = hotkeys::Table::read();
    loop {
        let msg = if i.gliding() {
            match rx.recv_timeout(Duration::from_millis(8)) {
                Ok(m) => Some(m),
                Err(RecvTimeoutError::Timeout) => None,
                Err(RecvTimeoutError::Disconnected) => break,
            }
        } else {
            match rx.recv() {
                Ok(m) => Some(m),
                Err(_) => break,
            }
        };
        match msg {
            Some(Msg::Resize(w, h)) => i.resize(w, h),
            Some(Msg::Frame(c)) => i.frame(Instant::now(), &c, &mut out),
            None => i.tick(Instant::now(), &mut out),
        }
    }
    i.lift_all(Instant::now(), &mut out);
}

impl TouchInjector for MacTouch {
    fn resize(&mut self, w: u16, h: u16) -> InputRes {
        self.tx
            .send(Msg::Resize(w, h))
            .map_err(|_| InputError::Failed("touchpad stopped".into()))
    }

    fn frame(&mut self, contacts: &[Contact]) -> InputRes {
        self.tx
            .send(Msg::Frame(contacts.to_vec()))
            .map_err(|_| InputError::Failed("touchpad stopped".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_chunks_at_20_utf16_units() {
        let s: Vec<u16> = "a".repeat(45).encode_utf16().collect();
        let c = text_chunks(&s, 20);
        assert_eq!(c.iter().map(|c| c.len()).collect::<Vec<_>>(), [20, 20, 5]);
    }

    #[test]
    fn a_surrogate_pair_is_never_split() {
        let s: Vec<u16> = format!("{}😀", "a".repeat(19)).encode_utf16().collect();
        let c = text_chunks(&s, 20);
        assert_eq!(c[0].len(), 19);
        assert_eq!(c[1].len(), 2);
    }
}
