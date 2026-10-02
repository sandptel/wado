//! The daemon's side of `wado-config`: reloading the file, telling viewers, answering edits.
//!
//! - [`watch`] — notices the files changed and reloads them.
//! - [`link`] — what a viewer is told, and what it may change.
//! - [`socket`] — `wado msg`, the local command line.

pub mod link;
pub mod socket;
pub mod watch;

use std::sync::OnceLock;

use tokio::sync::watch as tw;

/// The outcome of the last reload, broadcast to every connection.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Status {
    /// The last reload failed; the daemon runs on the config before it.
    pub error: Option<String>,
    /// Sections that changed but only take effect after a restart.
    pub restart: Vec<String>,
    /// Bumped on every reload, so an identical outcome still wakes subscribers.
    pub generation: u64,
}

fn channel() -> &'static tw::Sender<Status> {
    static TX: OnceLock<tw::Sender<Status>> = OnceLock::new();
    TX.get_or_init(|| tw::channel(Status::default()).0)
}

pub fn status() -> tw::Receiver<Status> {
    channel().subscribe()
}

pub fn current_status() -> Status {
    channel().borrow().clone()
}

fn publish(error: Option<String>, restart: Vec<String>) {
    channel().send_modify(|s| {
        s.error = error;
        s.restart = restart;
        s.generation += 1;
    });
}
