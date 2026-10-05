//! The daemon's shells: several per daemon, owned by it rather than by a connection.
//!
//! A connection dropping — a phone locking, a tunnel blinking — leaves every shell running. The
//! next viewer asks for the list and gets each one back with its scrollback, which is what makes
//! ssh from a phone on mobile data usable at all. A shell ends when it exits or its tab is closed.
//!
//! - [`access`] — which devices may use them.
//! - [`hosts`] — the ssh aliases on offer, which double as the allow-list.
//! - [`scrollback`] — the output kept for a reattach.

pub mod access;
pub mod hosts;
pub mod scrollback;

use std::{
    collections::BTreeMap,
    sync::{Mutex, OnceLock},
};

use tokio::sync::{broadcast, mpsc};
use wado_protocol::ShellInfo;

use crate::pty::Pty;
use scrollback::Scrollback;

/// More than this many open at once is a runaway client, not a person.
pub const MAX: usize = 8;

#[derive(Clone, Debug)]
pub enum Event {
    Output {
        id: u32,
        data: String,
    },
    Exit {
        id: u32,
    },
    /// The list changed: a shell opened, exited or closed.
    Changed,
}

struct Shell {
    pty: Option<Pty>,
    title: String,
    host: Option<String>,
    scroll: Scrollback,
}

#[derive(Default)]
struct Registry {
    next: u32,
    shells: BTreeMap<u32, Shell>,
}

fn reg() -> std::sync::MutexGuard<'static, Registry> {
    static REG: Mutex<Registry> = Mutex::new(Registry {
        next: 0,
        shells: BTreeMap::new(),
    });
    REG.lock().unwrap_or_else(|e| e.into_inner())
}

fn bus() -> &'static broadcast::Sender<Event> {
    static TX: OnceLock<broadcast::Sender<Event>> = OnceLock::new();
    TX.get_or_init(|| broadcast::channel(1024).0)
}

pub fn events() -> broadcast::Receiver<Event> {
    bus().subscribe()
}

/// Open a shell. Must run inside a tokio runtime: the output forwarder is a task.
pub fn open(cols: u16, rows: u16, host: Option<String>) -> Result<u32, String> {
    if let Some(h) = &host {
        if !hosts::list().iter().any(|a| a == h) {
            return Err(format!(
                "{h} is not a Host in this computer's ~/.ssh/config"
            ));
        }
    }
    if reg().shells.len() >= MAX {
        return Err(format!("{MAX} shells are open already — close one first"));
    }
    let (tx, mut rx) = mpsc::channel::<String>(64);
    let pty = Pty::open(cols, rows, host.as_deref(), tx).map_err(|e| e.to_string())?;
    let title = host.clone().unwrap_or_else(shell_name);
    let id = {
        let mut r = reg();
        r.next += 1;
        let id = r.next;
        r.shells.insert(
            id,
            Shell {
                pty: Some(pty),
                title,
                host,
                scroll: Scrollback::default(),
            },
        );
        id
    };
    tokio::spawn(async move {
        while let Some(data) = rx.recv().await {
            if let Some(s) = reg().shells.get_mut(&id) {
                s.scroll.push(&data);
            }
            let _ = bus().send(Event::Output { id, data });
        }
        // The channel closing means the reader saw EOF: the shell exited.
        if let Some(s) = reg().shells.get_mut(&id) {
            s.pty = None;
        }
        let _ = bus().send(Event::Exit { id });
        let _ = bus().send(Event::Changed);
    });
    let _ = bus().send(Event::Changed);
    Ok(id)
}

pub fn write(id: u32, data: &str) {
    let mut r = reg();
    if let Some(p) = r.shells.get_mut(&id).and_then(|s| s.pty.as_mut()) {
        if let Err(e) = p.write(data) {
            tracing::warn!(id, "pty write failed: {e}");
        }
    }
}

pub fn resize(id: u32, cols: u16, rows: u16) {
    if let Some(p) = reg().shells.get_mut(&id).and_then(|s| s.pty.as_mut()) {
        p.resize(cols, rows);
    }
}

/// Close a shell: dropping its PTY kills it, and SIGHUP takes its jobs with it.
pub fn close(id: u32) {
    if reg().shells.remove(&id).is_some() {
        let _ = bus().send(Event::Changed);
    }
}

pub fn list() -> Vec<ShellInfo> {
    reg()
        .shells
        .iter()
        .map(|(id, s)| ShellInfo {
            id: *id,
            title: s.title.clone(),
            host: s.host.clone(),
            alive: s.pty.is_some(),
        })
        .collect()
}

/// Each shell's scrollback, for a viewer that has just attached.
pub fn replay() -> Vec<(u32, String)> {
    reg()
        .shells
        .iter()
        .map(|(id, s)| (*id, s.scroll.text().to_string()))
        .collect()
}

fn shell_name() -> String {
    let program = wado_config::live::current()
        .shells
        .program
        .clone()
        .or_else(|| std::env::var("SHELL").ok())
        .unwrap_or_else(|| "sh".into());
    program.rsplit('/').next().unwrap_or("sh").to_string()
}
