//! The trackpad's compact touch frame: a [`crate::pad::PadMsg::Frame`] in `8 + 5n` bytes instead
//! of ~50 + 17n of JSON — about four times smaller, at 60+ frames a second.
//!
//! ```text
//! seq u32 | t u32 | n × (id u8, x u16, y u16)        all big-endian; n = (len − 8) / 5
//! ```
//!
//! **Negotiated, never assumed:** the phone's `ping` says `bin`, and only a daemon whose `pong`
//! says `bin` back is sent one. JSON stays the fallback for every older peer. On the direct link
//! it is a binary message on `pad-touch`; relayed, it is [`crate::pad::PadMsg::Bin`] (base64
//! inside the sealed `pad_in`), because the relay link carries text.
//!
//! The browser's encoder is `js/pad_link.js`; `scripts/pad-check.mjs` round-trips both.

use crate::pad::Contact;

/// The most contacts one frame may carry: the pad hands out ids 0..=9.
pub const MAX_CONTACTS: usize = 10;

/// Encode a frame.
pub fn encode(seq: u32, t: u32, c: &[Contact]) -> Vec<u8> {
    let mut out = Vec::with_capacity(8 + 5 * c.len().min(MAX_CONTACTS));
    out.extend_from_slice(&seq.to_be_bytes());
    out.extend_from_slice(&t.to_be_bytes());
    for &(id, x, y) in c.iter().take(MAX_CONTACTS) {
        out.push(id);
        out.extend_from_slice(&x.to_be_bytes());
        out.extend_from_slice(&y.to_be_bytes());
    }
    out
}

/// Decode a frame: `(seq, t, contacts)`, or `None` for anything that is not exactly one.
pub fn decode(b: &[u8]) -> Option<(u32, u32, Vec<Contact>)> {
    if b.len() < 8 || (b.len() - 8) % 5 != 0 || (b.len() - 8) / 5 > MAX_CONTACTS {
        return None;
    }
    let u32_at = |i: usize| u32::from_be_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]);
    let c = b[8..]
        .chunks_exact(5)
        .map(|f| {
            (
                f[0],
                u16::from_be_bytes([f[1], f[2]]),
                u16::from_be_bytes([f[3], f[4]]),
            )
        })
        .collect();
    Some((u32_at(0), u32_at(4), c))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pad::PadMsg;

    #[test]
    fn round_trips_and_is_about_four_times_smaller() {
        let c = vec![(0, 32768, 12345), (1, 0, 65535)];
        let b = encode(4_000_000_001, 123_456_789, &c);
        assert_eq!(b.len(), 8 + 5 * 2);
        assert_eq!(decode(&b), Some((4_000_000_001, 123_456_789, c.clone())));
        let json = serde_json::to_string(&PadMsg::Frame {
            seq: 4_000_000_001,
            t: 123_456_789,
            c,
        })
        .unwrap();
        assert!(json.len() >= 3 * b.len(), "{} vs {}", json.len(), b.len());
        // One finger, the common case: ~4×.
        let one = encode(70_000, 1_234_567, &[(0, 32768, 32768)]);
        let j1 = serde_json::to_string(&PadMsg::Frame {
            seq: 70_000,
            t: 1_234_567,
            c: vec![(0, 32768, 32768)],
        })
        .unwrap();
        assert!(
            j1.len() as f64 / one.len() as f64 >= 4.0,
            "{} vs {}",
            j1.len(),
            one.len()
        );
    }

    #[test]
    fn an_empty_frame_lifts_everything() {
        assert_eq!(decode(&encode(1, 2, &[])), Some((1, 2, vec![])));
    }

    #[test]
    fn the_exact_bytes_the_browser_writes() {
        // seq 7, t 1000, finger 0 at (100, 65535) — the JSON test's frame.
        let b = [0, 0, 0, 7, 0, 0, 3, 232, 0, 0, 100, 255, 255];
        assert_eq!(decode(&b), Some((7, 1000, vec![(0, 100, 65535)])));
        assert_eq!(encode(7, 1000, &[(0, 100, 65535)]), b);
    }

    #[test]
    fn anything_else_is_refused() {
        assert_eq!(decode(&[]), None);
        assert_eq!(decode(&[0; 7]), None);
        assert_eq!(decode(&[0; 9]), None, "a partial contact");
        assert_eq!(
            decode(&vec![0; 8 + 5 * 11]),
            None,
            "more fingers than a pad has"
        );
        assert!(decode(&vec![0; 8 + 5 * 10]).is_some());
    }
}
