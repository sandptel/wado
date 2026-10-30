//! The challenges this daemon hands out: random, for one device and one purpose, used once,
//! and gone after two minutes — so an answer cannot be replayed or carried to another device.
//!
//! A device may hold a few at once (two taps, a retry racing a slow prompt): the answer names the
//! one it was signed over, and that one is taken. Keeping only the newest made the first prompt's
//! answer "for another challenge" and left the person locked out (seen `2026-10-30`).

use std::sync::Mutex;
use std::time::{Duration, Instant};

pub const TTL: Duration = Duration::from_secs(120);
/// Live challenges kept per device and purpose; the oldest goes first.
const KEEP: usize = 4;

struct Pending {
    key: String,
    purpose: String,
    bytes: [u8; 32],
    at: Instant,
}

#[derive(Default)]
pub struct Challenges {
    pending: Vec<Pending>,
}

impl Challenges {
    /// A fresh challenge for `key` to `purpose`. Earlier live ones stay answerable, up to [`KEEP`].
    pub fn issue(&mut self, key: &str, purpose: &str, now: Instant) -> [u8; 32] {
        let bytes: [u8; 32] = rand::random();
        self.pending.retain(|p| now - p.at < TTL);
        let mine = |p: &Pending| p.key == key && p.purpose == purpose;
        while self.pending.iter().filter(|p| mine(p)).count() >= KEEP {
            let oldest = self.pending.iter().position(|p| mine(p)).expect("counted");
            self.pending.remove(oldest);
        }
        self.pending.push(Pending {
            key: key.into(),
            purpose: purpose.into(),
            bytes,
            at: now,
        });
        bytes
    }

    /// The live challenge for `key` to `purpose` that the answer was signed over, taken: a second
    /// answer finds nothing. Once one is answered the others are moot and go too.
    pub fn take(
        &mut self,
        key: &str,
        purpose: &str,
        signed: &[u8],
        now: Instant,
    ) -> Option<[u8; 32]> {
        let i = self
            .pending
            .iter()
            .position(|p| p.key == key && p.purpose == purpose && p.bytes == signed)?;
        let p = self.pending.remove(i);
        self.pending
            .retain(|q| q.key != key || q.purpose != purpose);
        (now - p.at < TTL).then_some(p.bytes)
    }
}

fn global() -> std::sync::MutexGuard<'static, Challenges> {
    static C: Mutex<Challenges> = Mutex::new(Challenges {
        pending: Vec::new(),
    });
    C.lock().unwrap_or_else(|e| e.into_inner())
}

pub fn issue(key: &str, purpose: &str) -> [u8; 32] {
    global().issue(key, purpose, Instant::now())
}

pub fn take(key: &str, purpose: &str, signed: &[u8]) -> Option<[u8; 32]> {
    global().take(key, purpose, signed, Instant::now())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn once_per_device_and_purpose_and_not_late() {
        let mut c = Challenges::default();
        let t = Instant::now();
        let a = c.issue("phone", "unlock", t);
        assert_eq!(c.take("tablet", "unlock", &a, t), None, "another device's");
        assert_eq!(
            c.take("phone", "register", &a, t),
            None,
            "another purpose's"
        );
        assert_eq!(
            c.take("phone", "unlock", &[0; 32], t),
            None,
            "one never issued"
        );
        assert_eq!(c.take("phone", "unlock", &a, t), Some(a));
        assert_eq!(c.take("phone", "unlock", &a, t), None, "used once");
        let x = c.issue("phone", "unlock", t);
        assert_eq!(c.take("phone", "unlock", &x, t + TTL), None, "expired");
    }

    #[test]
    fn two_taps_the_first_answer_still_counts_and_ends_both() {
        let mut c = Challenges::default();
        let t = Instant::now();
        let first = c.issue("phone", "unlock", t);
        let second = c.issue("phone", "unlock", t);
        assert_ne!(first, second);
        assert_eq!(c.take("phone", "unlock", &first, t), Some(first));
        assert_eq!(
            c.take("phone", "unlock", &second, t),
            None,
            "moot once one is answered"
        );
        for _ in 0..KEEP + 2 {
            c.issue("phone", "unlock", t);
        }
        assert_eq!(c.pending.len(), KEEP, "bounded");
    }
}
