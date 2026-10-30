//! One link's outbound queue on the lane: urgent frames go first and at once, bulk frames wait for
//! the budget, spare ones go only when nothing waits. Frames are **plaintext** here — the caller
//! seals in the order this hands them out, so jumping the queue never breaks the envelope's
//! counter order.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use wado_protocol::lane::Pace;

use super::classify::{Class, class};
use super::gauge::Gauge;

/// A safety stop, not the working limit: streaming producers hold themselves to
/// [`super::gauge::ROOM_BYTES`]. Past this the outbox takes no more until it drains, and whatever
/// is behind in the channel — urgent or not — waits. Only a producer that ignores the gauge gets
/// here.
pub const BACKLOG_BYTES: usize = 4 << 20;

/// What the next frame is, now.
#[derive(Debug, PartialEq)]
pub enum Next {
    Send(String),
    /// Nothing may go for this long (bulk is waiting on the budget).
    Wait(Duration),
    Idle,
}

pub struct Outbox {
    urgent: VecDeque<(String, bool)>,
    bulk: VecDeque<String>,
    bulk_bytes: usize,
    pace: Pace,
    epoch: Instant,
    /// Spare frames dropped since the last report.
    pub dropped: u64,
    gauge: Gauge,
}

/// What a frame costs on the wire once sealed: base64 (4/3) inside a small JSON wrapper. An
/// estimate on purpose — the daemon's budget sits under the relay's to absorb it.
pub fn wire_bytes(plain: usize) -> usize {
    plain.div_ceil(3) * 4 + 48
}

impl Default for Outbox {
    fn default() -> Self {
        Self::new(Pace::daemon())
    }
}

impl Outbox {
    pub fn new(pace: Pace) -> Self {
        Self {
            urgent: VecDeque::new(),
            bulk: VecDeque::new(),
            bulk_bytes: 0,
            pace,
            epoch: Instant::now(),
            dropped: 0,
            gauge: Gauge::default(),
        }
    }

    /// What producers on this link wait on (see [`Gauge::room`]).
    pub fn gauge(&self) -> Gauge {
        self.gauge.clone()
    }

    fn now(&self) -> f64 {
        self.epoch.elapsed().as_secs_f64()
    }

    fn queue_bulk(&mut self, text: String) {
        self.bulk_bytes += text.len();
        self.gauge.add(text.len());
        self.bulk.push_back(text);
    }

    pub fn push(&mut self, text: String) {
        match class(&text) {
            Class::Free => self.urgent.push_back((text, false)),
            Class::Urgent => self.urgent.push_back((text, true)),
            Class::Bulk => self.queue_bulk(text),
            // A log line is worth a lane that is idle, never one that is busy.
            Class::Spare => {
                if self.bulk.is_empty() && !self.pace.saturated(self.now()) {
                    self.queue_bulk(text);
                } else {
                    self.dropped += 1;
                }
            }
        }
    }

    /// May the caller pull another frame off its channel?
    pub fn wants_more(&self) -> bool {
        self.bulk_bytes < BACKLOG_BYTES
    }

    pub fn is_empty(&self) -> bool {
        self.urgent.is_empty() && self.bulk.is_empty()
    }

    /// The frame to send now, if any. Spends the budget for it.
    pub fn next(&mut self) -> Next {
        let now = self.now();
        if let Some((text, charged)) = self.urgent.pop_front() {
            if charged {
                self.pace.spend(now, wire_bytes(text.len()));
            }
            return Next::Send(text);
        }
        let Some(front) = self.bulk.front() else {
            return Next::Idle;
        };
        let cost = wire_bytes(front.len());
        let wait = self.pace.wait(now, cost);
        if wait > 0.0 {
            return Next::Wait(Duration::from_secs_f64(wait));
        }
        self.pace.spend(now, cost);
        let text = self.bulk.pop_front().unwrap_or_default();
        self.bulk_bytes -= text.len();
        self.gauge.sub(text.len());
        Next::Send(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(kind: &str, n: usize) -> String {
        format!(r#"{{"type":"{kind}","pad":"{}"}}"#, "x".repeat(n))
    }

    #[test]
    fn an_answer_overtakes_a_transfer_already_waiting() {
        let mut o = Outbox::new(Pace::new(10_000, 10_000));
        for _ in 0..4 {
            o.push(f("files_chunk", 6_000));
        }
        // The burst lets the first chunk go; the rest must wait.
        assert!(matches!(o.next(), Next::Send(t) if t.contains("files_chunk")));
        assert!(matches!(o.next(), Next::Wait(_)));
        // An SDP answer arrives behind them, and goes now.
        o.push(f("sdp_answer", 3_000));
        assert!(matches!(o.next(), Next::Send(t) if t.contains("sdp_answer")));
        assert!(matches!(o.next(), Next::Wait(_)));
    }

    #[test]
    fn bulk_keeps_its_own_order() {
        let mut o = Outbox::new(Pace::new(1_000_000, 1_000_000));
        o.push(f("pty_output", 10));
        o.push(f("pty_exit", 10));
        assert!(matches!(o.next(), Next::Send(t) if t.contains("pty_output")));
        assert!(matches!(o.next(), Next::Send(t) if t.contains("pty_exit")));
        assert_eq!(o.next(), Next::Idle);
    }

    #[test]
    fn log_lines_go_only_on_an_idle_lane() {
        let mut o = Outbox::new(Pace::new(10_000, 10_000));
        o.push(f("log", 10));
        assert!(matches!(o.next(), Next::Send(t) if t.contains("log")));
        o.push(f("pty_output", 20_000));
        o.push(f("log", 10));
        assert_eq!(o.dropped, 1);
    }

    #[test]
    fn the_relays_own_frames_cost_nothing() {
        let mut o = Outbox::new(Pace::new(1_000, 1_000));
        for _ in 0..50 {
            o.push(r#"{"type":"pong"}"#.into());
            assert!(matches!(o.next(), Next::Send(_)));
        }
        o.push(f("pty_output", 500));
        assert!(matches!(o.next(), Next::Send(_)), "budget untouched");
    }

    #[test]
    fn the_gauge_follows_the_bulk_backlog() {
        let mut o = Outbox::new(Pace::new(1_000_000, 1_000_000));
        let g = o.gauge();
        o.push(f("pty_output", 100));
        o.push(f("sdp_answer", 100));
        assert!(g.queued() > 100 && g.queued() < 200, "only bulk counts");
        while o.next() != Next::Idle {}
        assert_eq!(g.queued(), 0);
        while o.wants_more() {
            o.push(f("pty_output", 64 * 1024));
        }
        assert!(o.bulk_bytes >= BACKLOG_BYTES && g.busy());
    }
}
