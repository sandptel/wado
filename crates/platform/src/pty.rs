//! Shells on a pseudo-terminal: a real terminal, so job control, line editing, colour and
//! full-screen programs all work.

use std::path::PathBuf;

use tokio::sync::mpsc;

use crate::Res;

/// What runs on the terminal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Program {
    /// This login shell (a program path or name), as a login shell.
    Shell(String),
    /// `ssh <alias>` — an alias the caller checked against the user's own ssh config. One
    /// argument, never through a shell.
    Ssh(String),
}

/// A terminal to open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PtyRequest {
    pub cols: u16,
    pub rows: u16,
    pub program: Program,
    /// Where it starts.
    pub cwd: Option<PathBuf>,
}

/// Opens terminals.
pub trait PtySpawner: Send + Sync {
    /// Start `req` on a new terminal; its output goes to `out` as text, in order, split only on
    /// whole characters. The output lane closing ends the reader.
    fn spawn(&self, req: PtyRequest, out: mpsc::Sender<String>) -> Res<Box<dyn PtySession>>;
}

/// A running terminal. Dropping it ends the program and everything on its terminal.
pub trait PtySession: Send {
    /// Keystrokes to the program.
    fn write(&mut self, data: &str) -> Res<()>;
    /// The terminal changed size.
    fn resize(&mut self, cols: u16, rows: u16);
    /// `Some(code)` once the program exited (`Some(None)`: it is gone, its code unknown).
    fn exited(&mut self) -> Option<Option<i32>>;
}
