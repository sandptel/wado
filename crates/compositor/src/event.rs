//! Typed session events the compositor emits to the server. Kept in a dedicated module
//! (one job: define the event envelope) so neither `headless` nor the server need to
//! know each other's internals.

use tokio::sync::mpsc;
use wado_protocol::EncoderReport;

/// An event the compositor pushes to the server during a live session.
pub enum SessionEvent {
    /// The active pipeline tier was downgraded at runtime (encode failure → next tier).
    EncoderChanged(EncoderReport),
    /// The session ended abnormally (render panic, or all encoder tiers exhausted).
    /// The server fans this out as a named `event: session` SSE frame so the client
    /// can display a "Session ended — reconnect" banner instead of a silent frozen video.
    SessionEnded { reason: String },
}

/// Sender half of the session-event channel (compositor end).
pub type EventSender = mpsc::Sender<SessionEvent>;
