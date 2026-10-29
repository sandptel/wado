//! Evening out touch frames that arrived through the relay.
//!
//! The relay path is TCP: a stall delivers a burst of frames at once, and played as they land
//! the cursor would freeze and then leap. Each frame instead plays at its own client time plus
//! the smallest delay seen recently (the sliding minimum of arrival − client time over
//! [`WINDOW_MS`]) plus [`PLAYOUT_DELAY_MS`]. A frame already late plays at once. Of a run of late
//! frames that hold the same fingers only the last matters (positions are absolute), so the run
//! collapses to it; a frame where a finger lands or lifts is never dropped, so a tap inside a
//! burst still clicks.
//!
//! Direct frames never come here: on a direct link they play the moment they arrive.

use std::collections::VecDeque;

use wado_protocol::pad::Contact;

/// Added on top of the best recent delay, to absorb ordinary jitter.
pub const PLAYOUT_DELAY_MS: u64 = 24;
/// How far back the best delay is remembered.
pub const WINDOW_MS: u64 = 2000;

#[derive(Default)]
pub struct Playout {
    /// (due ms, frame), in arrival order.
    queue: VecDeque<(u64, Vec<Contact>)>,
    /// (arrival ms, arrival − unwrapped client time), oldest first.
    offsets: VecDeque<(u64, i64)>,
    /// The last client time seen, raw and unwrapped across u32 wrap.
    last_t: Option<(u32, i64)>,
}

impl Playout {
    pub fn new() -> Playout {
        Playout::default()
    }

    /// A relayed frame stamped `t` (client ms, wrapping) arrived at `now_ms`.
    pub fn push(&mut self, now_ms: u64, t: u32, c: Vec<Contact>) {
        let t = match self.last_t {
            Some((raw, unwrapped)) => unwrapped + t.wrapping_sub(raw) as i32 as i64,
            None => t as i64,
        };
        self.last_t = Some((t as u32, t));
        let offset = now_ms as i64 - t;
        while self
            .offsets
            .front()
            .is_some_and(|&(at, _)| at + WINDOW_MS < now_ms)
        {
            self.offsets.pop_front();
        }
        self.offsets.push_back((now_ms, offset));
        let best = self.offsets.iter().map(|&(_, o)| o).min().unwrap_or(offset);
        let due = (t + best).max(0) as u64 + PLAYOUT_DELAY_MS;
        self.queue.push_back((due, c));
    }

    /// Every frame due by `now_ms`, in order, with same-finger runs collapsed to their last.
    pub fn due(&mut self, now_ms: u64) -> Vec<Vec<Contact>> {
        let mut out: Vec<Vec<Contact>> = Vec::new();
        while self.queue.front().is_some_and(|&(due, _)| due <= now_ms) {
            let (_, frame) = self.queue.pop_front().unwrap();
            match out.last_mut() {
                Some(last) if same_fingers(last, &frame) => *last = frame,
                _ => out.push(frame),
            }
        }
        out
    }

    /// Forget queued frames (the pad was resized or lifted).
    pub fn clear(&mut self) {
        self.queue.clear();
    }
}

fn same_fingers(a: &[Contact], b: &[Contact]) -> bool {
    a.len() == b.len() && a.iter().all(|x| b.iter().any(|y| y.0 == x.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steady_frames_come_out_spaced() {
        let mut p = Playout::new();
        p.push(1000, 0, vec![(0, 1, 1)]);
        p.push(1016, 16, vec![(0, 2, 2)]);
        assert_eq!(p.due(1023).len(), 0);
        assert_eq!(p.due(1024), vec![vec![(0, 1, 1)]]);
        assert_eq!(p.due(1040), vec![vec![(0, 2, 2)]]);
    }

    #[test]
    fn burst_collapses_but_keeps_taps() {
        let mut p = Playout::new();
        p.push(1000, 0, vec![(0, 1, 1)]); // baseline offset 1000
        // a 300 ms stall, then everything at once
        let burst = [
            vec![(0, 2, 2)],
            vec![(0, 3, 3)],
            vec![],
            vec![(1, 9, 9)],
            vec![],
        ];
        for (i, f) in burst.into_iter().enumerate() {
            p.push(1316, 16 * (i as u32 + 1), f);
        }
        let out = p.due(2000);
        // the moves collapse into the last one before the lift; the tap (down, lift) survives
        assert_eq!(out, vec![vec![(0, 3, 3)], vec![], vec![(1, 9, 9)], vec![]]);
    }

    #[test]
    fn client_clock_wrap_does_not_stall_playout() {
        let mut p = Playout::new();
        p.push(1000, u32::MAX - 7, vec![(0, 1, 1)]);
        p.push(1016, 8, vec![(0, 2, 2)]); // 16 ms later on the client, across the wrap
        assert_eq!(p.due(1024), vec![vec![(0, 1, 1)]]);
        assert_eq!(p.due(1040), vec![vec![(0, 2, 2)]]);
    }
}
