//! The client's side of the relay lane: what waits, what jumps, and sealing at the last moment.
//!
//! The envelope's counters are strict, so the outbox holds **plaintext** and seals each frame as
//! it leaves: an urgent frame that jumps the queue still gets the next counter, and a frame queued
//! across a reconnect is sealed with the new link's keys, never the old ones. Budget: the daemon's
//! own numbers (`wado_protocol::lane`, 56 KiB/s with a 1.25 MiB burst). Urgent frames go at once
//! and are still counted; bulk frames wait for the budget; spare frames go only when the lane is
//! idle and are dropped otherwise. The class mapping mirrors `crates/server/src/lane/classify.rs`.

use std::collections::VecDeque;

use wado_protocol::lane::Pace;

use crate::e2e::seal::Sealer;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Class {
    /// Small and time-critical: signalling, verbs, polls a person is waiting on.
    Urgent,
    /// Big or streaming: uploads, clipboard pictures, pty input bursts.
    Bulk,
    /// Only when idle: the client's log lines.
    Spare,
}

/// A message type's class, from the client's side of the lane.
pub fn class_of(kind: &str) -> Class {
    match kind {
        "files_in" | "clip_push" | "clipboard_set" | "pty_input" => Class::Bulk,
        "client_log" => Class::Spare,
        _ => Class::Urgent,
    }
}

/// The length of the sealed frame for `plain` bytes at counter `n`: base64url of the plaintext
/// plus the 16-byte tag, inside `{"type":"sealed","n":<n>,"c":"…"}`.
fn sealed_len(plain: usize, n: u64) -> usize {
    let c = (plain + 16) * 4 / 3 + usize::from((plain + 16) % 3 != 0);
    r#"{"type":"sealed","n":,"c":""}"#.len() + n.to_string().len() + c
}

struct Item {
    text: String,
    /// Kept across a reconnect and sent under the new keys; otherwise dropped with the link.
    resend: bool,
}

/// What [`Outbox::next`] gives.
#[derive(Debug, PartialEq)]
pub enum Next {
    /// A sealed frame to send now.
    Frame(String),
    /// Nothing may go for this many seconds.
    Wait(f64),
    /// Nothing to send, or no keys yet.
    Idle,
}

pub struct Outbox {
    urgent: VecDeque<Item>,
    bulk: VecDeque<Item>,
    spare: VecDeque<Item>,
    pace: Pace,
    sealer: Option<Sealer>,
}

impl Default for Outbox {
    fn default() -> Self {
        Self::new()
    }
}

impl Outbox {
    pub fn new() -> Self {
        Outbox {
            urgent: VecDeque::new(),
            bulk: VecDeque::new(),
            spare: VecDeque::new(),
            pace: Pace::daemon(),
            sealer: None,
        }
    }

    pub fn push(&mut self, text: String, class: Class, resend: bool) {
        let item = Item { text, resend };
        match class {
            Class::Urgent => self.urgent.push_back(item),
            Class::Bulk => self.bulk.push_back(item),
            Class::Spare => self.spare.push_back(item),
        }
    }

    /// New keys (a fresh handshake): what is still queued is sealed with these.
    pub fn rekey(&mut self, sealer: Sealer) {
        self.sealer = Some(sealer);
    }

    /// The link dropped: no keys until the next handshake, and what was not to be resent goes.
    pub fn link_lost(&mut self) {
        self.sealer = None;
        for q in [&mut self.urgent, &mut self.bulk, &mut self.spare] {
            q.retain(|i| i.resend);
        }
    }

    pub fn is_empty(&self) -> bool {
        self.urgent.is_empty() && self.bulk.is_empty() && self.spare.is_empty()
    }

    /// The next frame that may leave at `now` (seconds, any monotonic clock).
    pub fn next(&mut self, now: f64) -> Next {
        let Some(sealer) = self.sealer.as_mut() else {
            return Next::Idle;
        };
        if let Some(item) = self.urgent.pop_front() {
            let frame = sealer.seal(item.text.as_bytes());
            self.pace.spend(now, frame.len());
            return Next::Frame(frame);
        }
        if let Some(item) = self.bulk.front() {
            let wait = self
                .pace
                .wait(now, sealed_len(item.text.len(), sealer.next_n()));
            if wait > 0.0 {
                return Next::Wait(wait);
            }
            let item = self.bulk.pop_front().expect("front was there");
            let frame = sealer.seal(item.text.as_bytes());
            self.pace.spend(now, frame.len());
            return Next::Frame(frame);
        }
        while let Some(item) = self.spare.pop_front() {
            if self
                .pace
                .wait(now, sealed_len(item.text.len(), sealer.next_n()))
                > 0.0
            {
                continue;
            }
            let frame = sealer.seal(item.text.as_bytes());
            self.pace.spend(now, frame.len());
            return Next::Frame(frame);
        }
        Next::Idle
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::e2e::seal::Opener;

    fn sealed_n(frame: &str) -> u64 {
        let v: serde_json::Value = serde_json::from_str(frame).unwrap();
        v["n"].as_u64().unwrap()
    }

    fn take(o: &mut Outbox, now: f64) -> Vec<String> {
        let mut out = Vec::new();
        while let Next::Frame(f) = o.next(now) {
            out.push(f);
        }
        out
    }

    #[test]
    fn urgent_jumps_bulk() {
        let mut o = Outbox::new();
        o.rekey(Sealer::new([1; 32]));
        o.push("bulk-1".into(), Class::Bulk, true);
        o.push("bulk-2".into(), Class::Bulk, true);
        o.push("urgent".into(), Class::Urgent, true);
        let mut op = Opener::new([1; 32]);
        let order: Vec<Vec<u8>> = take(&mut o, 0.0)
            .iter()
            .map(|f| op.open(f).unwrap())
            .collect();
        assert_eq!(order, [&b"urgent"[..], b"bulk-1", b"bulk-2"]);
        assert_eq!(class_of("sdp_offer"), Class::Urgent);
        assert_eq!(class_of("host_get"), Class::Urgent);
        assert_eq!(class_of("files_in"), Class::Bulk);
        assert_eq!(class_of("client_log"), Class::Spare);
    }

    #[test]
    fn outbox_seals_at_dequeue() {
        // Counters follow the order frames leave, not the order they were queued.
        let mut o = Outbox::new();
        o.push("b".into(), Class::Bulk, true);
        assert_eq!(o.next(0.0), Next::Idle, "nothing leaves before the keys");
        o.rekey(Sealer::new([2; 32]));
        o.push("u1".into(), Class::Urgent, true);
        o.push("u2".into(), Class::Urgent, true);
        let frames = take(&mut o, 0.0);
        let ns: Vec<u64> = frames.iter().map(|f| sealed_n(f)).collect();
        assert_eq!(ns, [0, 1, 2]);
        let mut op = Opener::new([2; 32]);
        for f in &frames {
            op.open(f).expect("contiguous counters open in order");
        }
    }

    #[test]
    fn outbox_reseals_after_rekey() {
        let mut o = Outbox::new();
        o.rekey(Sealer::new([3; 32]));
        o.push("first".into(), Class::Urgent, true);
        let f0 = take(&mut o, 0.0);
        assert_eq!(f0.len(), 1);
        // Queued while the link is down: one to resend, one not.
        o.link_lost();
        o.push("resend-me".into(), Class::Urgent, true);
        o.push("a-poll".into(), Class::Urgent, false);
        o.link_lost();
        assert_eq!(o.next(0.0), Next::Idle);
        // A new handshake: the survivor goes under the new keys, from counter 0.
        o.rekey(Sealer::new([4; 32]));
        let f1 = take(&mut o, 0.0);
        assert_eq!(f1.len(), 1);
        assert_eq!(sealed_n(&f1[0]), 0);
        assert_eq!(Opener::new([4; 32]).open(&f1[0]).unwrap(), b"resend-me");
        assert!(
            Opener::new([3; 32]).open(&f1[0]).is_err(),
            "never the old keys"
        );
    }

    #[test]
    fn pace_holds_budget() {
        use wado_protocol::lane::{DAEMON_BURST_BYTES, DAEMON_BYTES_PER_SEC};
        let mut o = Outbox::new();
        o.rekey(Sealer::new([5; 32]));
        let chunk = "x".repeat(16 * 1024);
        for _ in 0..128 {
            o.push(chunk.clone(), Class::Bulk, true);
        }
        // 2 MiB queued. Step a clock and check the sent total against burst + rate × t.
        let (mut t, mut sent) = (0.0f64, 0usize);
        while t < 40.0 {
            loop {
                match o.next(t) {
                    Next::Frame(f) => sent += f.len(),
                    Next::Wait(_) | Next::Idle => break,
                }
            }
            let budget = f64::from(DAEMON_BURST_BYTES) + f64::from(DAEMON_BYTES_PER_SEC) * t;
            assert!(sent as f64 <= budget + 1.0, "t={t}: {sent} > {budget}");
            t += 0.25;
        }
        assert!(o.is_empty(), "everything goes, in time: nothing is dropped");
    }

    #[test]
    fn sealed_len_is_exact() {
        for len in [0usize, 1, 2, 3, 100, 16 * 1024] {
            for n in [0u64, 9, 10, 12345] {
                let mut s = Sealer::new([6; 32]);
                for _ in 0..n {
                    s.seal(b"");
                }
                assert_eq!(
                    s.seal(&vec![b'x'; len]).len(),
                    sealed_len(len, n),
                    "{len} {n}"
                );
            }
        }
    }
}
