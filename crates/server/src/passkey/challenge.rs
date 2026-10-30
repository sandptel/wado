//! The challenges this daemon hands out: random, for one device and one purpose, used once,
//! and gone after two minutes — so an answer cannot be replayed or carried to another device.

use std::sync::Mutex;
use std::time::{Duration, Instant};

pub const TTL: Duration = Duration::from_secs(120);

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
    /// A fresh challenge for `key` to `purpose`; any earlier one for the same is dropped.
    pub fn issue(&mut self, key: &str, purpose: &str, now: Instant) -> [u8; 32] {
        let bytes: [u8; 32] = rand::random();
        self.pending
            .retain(|p| (p.key != key || p.purpose != purpose) && now - p.at < TTL);
        self.pending.push(Pending {
            key: key.into(),
            purpose: purpose.into(),
            bytes,
            at: now,
        });
        bytes
    }

    /// The live challenge for `key` to `purpose`, taken: a second answer finds nothing.
    pub fn take(&mut self, key: &str, purpose: &str, now: Instant) -> Option<[u8; 32]> {
        let i = self
            .pending
            .iter()
            .position(|p| p.key == key && p.purpose == purpose)?;
        let p = self.pending.swap_remove(i);
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

pub fn take(key: &str, purpose: &str) -> Option<[u8; 32]> {
    global().take(key, purpose, Instant::now())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn once_per_device_and_purpose_and_not_late() {
        let mut c = Challenges::default();
        let t = Instant::now();
        let a = c.issue("phone", "unlock", t);
        assert_eq!(c.take("tablet", "unlock", t), None, "another device's");
        assert_eq!(c.take("phone", "register", t), None, "another purpose's");
        assert_eq!(c.take("phone", "unlock", t), Some(a));
        assert_eq!(c.take("phone", "unlock", t), None, "used once");
        let b = c.issue("phone", "unlock", t);
        let b2 = c.issue("phone", "unlock", t);
        assert_ne!(b, b2);
        assert_eq!(c.take("phone", "unlock", t), Some(b2), "the newest only");
        c.issue("phone", "unlock", t);
        assert_eq!(c.take("phone", "unlock", t + TTL), None, "expired");
    }
}
