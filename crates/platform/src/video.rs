//! Video: a backend that renders and encodes, declared as a runner with the thread it needs.
//!
//! The model says what the picture is — wado's own session (the Smithay compositor today), a
//! mirror of an existing desktop, or a virtual display added to it. The thread says where the
//! runner must run: `Main` for stacks bound to the process's main thread (AppKit on macOS, a GUI
//! toolkit's loop), `Any` otherwise. The caller owns that thread and calls [`Runner::run`] on it.
//!
//! Frames and handles are the backend's own types (the compositor's typed command/frame
//! boundary for the Smithay backend), so the server keeps speaking that boundary and never names
//! a toolkit's types.

use tokio::sync::mpsc;

use crate::Res;

/// What the picture is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Model {
    /// wado's own compositor: apps run inside the session.
    OwnSession,
    /// An existing desktop, mirrored.
    Mirror,
    /// A display added to an existing desktop.
    VirtualDisplay,
}

/// Where a backend's runner must run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Thread {
    /// Any thread the caller gives it.
    Any,
    /// The process's main thread.
    Main,
}

/// One video encoder a backend can use — found by opening it (invariant 6), not by its name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Encoder {
    pub name: &'static str,
    pub hardware: bool,
}

/// A built backend's loop. Runs on the calling thread until the backend stops (a signal, a
/// shutdown command).
pub trait Runner {
    fn run(self: Box<Self>) -> Res<()>;
}

/// A source of encoded video.
pub trait VideoBackend {
    /// What it sends down the frame lane.
    type Frame: Send + 'static;
    /// How the caller drives it: commands, input, its published state.
    type Handles;

    fn model(&self) -> Model;
    fn thread(&self) -> Thread;
    /// The encoders that opened here. Probing opens each one, so the first call may take a
    /// moment; the answer is cached.
    fn encoders(&self) -> Vec<Encoder>;
    /// Make the backend: frames go to `frames`. Returns its loop, for the caller to run on the
    /// thread [`VideoBackend::thread`] asks for, and the handles that drive it.
    fn build(
        self: Box<Self>,
        frames: mpsc::Sender<Self::Frame>,
    ) -> Res<(Box<dyn Runner>, Self::Handles)>;
}
