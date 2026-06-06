//! A bridge from wado's `tracing` output to the web client's live log panel.
//!
//! [`LogBus`] is a `tracing_subscriber::Layer`: every event the subscriber lets
//! through is formatted to a single line and (a) broadcast to all connected
//! `/events` SSE listeners and (b) appended to a small ring buffer so a freshly
//! opened panel can backfill recent history.
//!
//! Line wire format (the web client splits on the first two `|`):
//! `LEVEL|HH:MM:SS|target: message field=value …`
//!
//! Note: libx264 prints some lines straight to the C stderr, bypassing `tracing`,
//! so those do not appear here — only Rust-side `tracing` events do.

use std::collections::VecDeque;
use std::fmt::Write as _;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use tokio::sync::broadcast;
use tracing::Subscriber;
use tracing::field::{Field, Visit};
use tracing_subscriber::layer::{Context, Layer};

const RING_CAPACITY: usize = 200;
const BROADCAST_CAPACITY: usize = 512;

#[derive(Clone)]
pub struct LogBus {
    tx: broadcast::Sender<String>,
    ring: Arc<Mutex<VecDeque<String>>>,
}

impl LogBus {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(BROADCAST_CAPACITY);
        Self { tx, ring: Arc::new(Mutex::new(VecDeque::with_capacity(RING_CAPACITY))) }
    }

    /// Subscribe to live log lines (new lines only).
    pub fn subscribe(&self) -> broadcast::Receiver<String> {
        self.tx.subscribe()
    }

    /// Recent buffered lines, oldest first — sent to a panel when it connects.
    pub fn backfill(&self) -> Vec<String> {
        self.ring.lock().unwrap().iter().cloned().collect()
    }

    fn emit(&self, line: String) {
        {
            let mut ring = self.ring.lock().unwrap();
            if ring.len() == RING_CAPACITY {
                ring.pop_front();
            }
            ring.push_back(line.clone());
        }
        // Err only means no live subscribers — that's fine.
        let _ = self.tx.send(line);
    }
}

impl Default for LogBus {
    fn default() -> Self {
        Self::new()
    }
}

impl<S: Subscriber> Layer<S> for LogBus {
    fn on_event(&self, event: &tracing::Event<'_>, _ctx: Context<'_, S>) {
        let meta = event.metadata();
        let mut visitor = LineVisitor::default();
        event.record(&mut visitor);
        // `text` is `target: message field=value …`; the wire delimiter contract
        // (`LEVEL|HH:MM:SS|text`) lives in `wado_protocol::logfmt`, shared with the
        // client's parser.
        let text = format!("{}: {}{}", meta.target(), visitor.message, visitor.fields);
        let line = wado_protocol::logfmt::format_line(&meta.level().to_string(), &hms(), &text);
        self.emit(line.replace('\n', " "));
    }
}

/// Collects the `message` field plus any structured key=value fields.
#[derive(Default)]
struct LineVisitor {
    message: String,
    fields: String,
}

impl Visit for LineVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            // `message` carries fmt::Arguments; its Debug is the plain text.
            self.message = format!("{value:?}");
        } else {
            let _ = write!(self.fields, " {}={:?}", field.name(), value);
        }
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            self.message = value.to_string();
        } else {
            let _ = write!(self.fields, " {}={}", field.name(), value);
        }
    }
}

/// UTC `HH:MM:SS` without pulling in a date/time crate.
fn hms() -> String {
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    format!("{:02}:{:02}:{:02}", (secs / 3600) % 24, (secs / 60) % 60, secs % 60)
}
