//! One link's outbound queue on the lane: urgent frames go first and at once, bulk frames wait for
//! the budget, spare ones go only when nothing waits. Frames are **plaintext** here — the caller
//! seals in the order this hands them out, so jumping the queue never breaks the envelope's
//! counter order.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use tracing::warn;
use wado_protocol::lane::Pace;

use super::classify::{Class, class};
use super::gauge::Gauge;

/// A safety stop, not the working limit: streaming producers hold themselves to
/// [`super::gauge::ROOM_BYTES`]. Past this a new bulk frame is dropped, and said so — never held
/// in the channel, where it would hold everything behind it, urgent or not, and the relay loop
/// sending its `pong` with them (the hang of 2026-10-30). Only a producer that ignores the gauge
/// gets here; a device leaving empties it ([`Outbox::set_generation`]), so stale bulk never piles
/// up across reconnects.
pub const BACKLOG_BYTES: usize = 16 << 20;

/// What the next frame is, now.
#[derive(Debug, PartialEq)]
pub enum Next {
    Send(String),
    /// Nothing may go for this long (bulk is waiting on the budget).
    Wait(Duration),
    Idle,
}

pub struct Outbox {
    urgent: VecDeque<(String, bool, Instant)>,
    bulk: VecDeque<(String, Instant)>,
    bulk_bytes: usize,
    pace: Pace,
    epoch: Instant,
    /// Spare frames dropped since the last report.
    pub dropped: u64,
    /// Bulk frames dropped because [`BACKLOG_BYTES`] already waited.
    pub shed: u64,
    gauge: Gauge,
    /// The link generation (`crate::e2e::link::Link::generation`) what is queued belongs to.
    generation: u64,
    /// How long the frame [`Outbox::next`] last handed out had been queued (`crate::slow`).
    pub last_age: Duration,
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
            shed: 0,
            gauge: Gauge::default(),
            generation: 0,
            last_age: Duration::ZERO,
        }
    }

    /// The device on the link is now `generation`. Anything queued for the one before — it was
    /// sealed for nobody yet — is dropped: it must not be sealed for the next device (a shell's
    /// output after a takeover), and it is not worth the lane's time. The relay's own frames
    /// (uncharged) are kept: they are not a device's.
    pub fn set_generation(&mut self, generation: u64) {
        if generation == self.generation {
            return;
        }
        self.generation = generation;
        self.urgent.retain(|(_, charged, _)| !charged);
        self.gauge.sub(self.bulk_bytes);
        self.bulk.clear();
        self.bulk_bytes = 0;
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
        self.bulk.push_back((text, Instant::now()));
    }

    pub fn push(&mut self, text: String) {
        match class(&text) {
            Class::Free => self.urgent.push_back((text, false, Instant::now())),
            Class::Urgent => self.urgent.push_back((text, true, Instant::now())),
            Class::Bulk if self.bulk_bytes >= BACKLOG_BYTES => {
                warn!(
                    "lane: {} ({} KB) dropped — {} MB of bulk already waits on the lane",
                    crate::e2e::link::kind(&text).unwrap_or("?"),
                    text.len() / 1024,
                    self.bulk_bytes >> 20
                );
                self.shed += 1;
            }
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

    pub fn is_empty(&self) -> bool {
        self.urgent.is_empty() && self.bulk.is_empty()
    }

    /// `(urgent frames, bulk frames, bulk bytes)` waiting now — for a slow frame's log line.
    pub fn depth(&self) -> (usize, usize, usize) {
        (self.urgent.len(), self.bulk.len(), self.bulk_bytes)
    }

    /// The frame to send now, if any. Spends the budget for it.
    pub fn next(&mut self) -> Next {
        let now = self.now();
        if let Some((text, charged, at)) = self.urgent.pop_front() {
            if charged {
                self.pace.spend(now, wire_bytes(text.len()));
            }
            self.last_age = at.elapsed();
            return Next::Send(text);
        }
        let Some((front, _)) = self.bulk.front() else {
            return Next::Idle;
        };
        let cost = wire_bytes(front.len());
        let wait = self.pace.wait(now, cost);
        if wait > 0.0 {
            return Next::Wait(Duration::from_secs_f64(wait));
        }
        self.pace.spend(now, cost);
        let Some((text, at)) = self.bulk.pop_front() else {
            return Next::Idle;
        };
        self.last_age = at.elapsed();
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
    fn a_new_device_never_gets_what_was_queued_for_the_last() {
        let mut o = Outbox::new(Pace::new(1_000, 1_000));
        let g = o.gauge();
        o.set_generation(1);
        o.push(f("pty_output", 50_000));
        o.push(f("sdp_answer", 10));
        o.push(r#"{"type":"peer_accept","room_id":"r2"}"#.into());
        o.set_generation(2);
        // The relay's own verdict stays; the old device's answer and output are gone.
        assert!(matches!(o.next(), Next::Send(t) if t.contains("peer_accept")));
        assert_eq!(o.next(), Next::Idle);
        assert_eq!(g.queued(), 0);
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
        while o.bulk_bytes < BACKLOG_BYTES {
            o.push(f("pty_output", 64 * 1024));
        }
        assert!(g.busy());
        // Past the stop, bulk is dropped rather than held; an answer still gets in.
        let full = o.bulk_bytes;
        o.push(f("clip_data", 1 << 20));
        o.push(f("sdp_answer", 10));
        assert_eq!((o.bulk_bytes, o.shed), (full, 1));
        assert!(matches!(o.next(), Next::Send(t) if t.contains("sdp_answer")));
    }
}
