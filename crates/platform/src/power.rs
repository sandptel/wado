//! Power: keeping the computer awake, and locking it or putting it to sleep.

use tokio::sync::watch;

use crate::{Fut, Res};

/// Keeping the computer from sleeping, for as long as asked.
pub trait PowerInhibit: Send + Sync {
    /// Held now. Published on every change, including the hold ending by itself.
    fn state(&self) -> watch::Receiver<bool>;
    /// Hold, or let go.
    fn set(&self, on: bool) -> Res<()>;
    /// Notice a hold that ended by itself. Cheap; no I/O beyond a process check.
    fn check(&self) {}
}

/// One thing [`SystemPower`] can do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PowerOp {
    Lock,
    Suspend,
}

/// Lock the screen, or sleep.
pub trait SystemPower: Send + Sync {
    /// `op` can be done here (its tool is installed, its service reachable) — or why not.
    fn can(&self, op: PowerOp) -> Res<()>;
    fn run(&self, op: PowerOp) -> Fut<'_, Res<()>>;
}
