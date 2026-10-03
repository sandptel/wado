//! The media channel's chunk format — one SCTP message per network packet.
//!
//! ```text
//!   0      kind   0 = video, 1 = audio
//!   1      flags  bit 0: keyframe
//!   2..6   seq    u32 BE, per kind — a frame (video) or a packet (audio)
//!   6..8   idx    u16 BE, chunk index within the frame
//!   8..10  count  u16 BE, chunks in the frame
//!   10..18 ts_us  u64 BE, daemon clock: one clock for audio and video, which is what the
//!                 receiver syncs them on (A/V sync is mandatory — Decision Log 2026-10-03)
//!   18..   payload
//! ```
//!
//! Small chunks, not one message per frame: the channel is unreliable (no retransmits), and SCTP
//! drops a whole message when any of its fragments is lost — a 22 KB keyframe as one message is
//! sixteen chances to lose all of it. Chunked, a loss costs one chunk, which NACK can re-ask for.

pub const HEADER: usize = 18;
/// Payload per chunk: header + payload stays under a typical path MTU after DTLS/SCTP/UDP/IP.
pub const PAYLOAD: usize = 1100;

pub const VIDEO: u8 = 0;
pub const AUDIO: u8 = 1;

/// Split one frame into chunks, header first.
pub fn chunks(kind: u8, key: bool, seq: u32, ts_us: u64, data: &[u8]) -> Vec<Vec<u8>> {
    let parts: Vec<&[u8]> = if data.is_empty() {
        vec![&[][..]]
    } else {
        data.chunks(PAYLOAD).collect()
    };
    let count = parts.len() as u16;
    parts
        .into_iter()
        .enumerate()
        .map(|(i, p)| {
            let mut c = Vec::with_capacity(HEADER + p.len());
            c.push(kind);
            c.push(u8::from(key));
            c.extend_from_slice(&seq.to_be_bytes());
            c.extend_from_slice(&(i as u16).to_be_bytes());
            c.extend_from_slice(&count.to_be_bytes());
            c.extend_from_slice(&ts_us.to_be_bytes());
            c.extend_from_slice(p);
            c
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_frame_splits_and_every_chunk_says_where_it_belongs() {
        let data: Vec<u8> = (0..2500u32).map(|i| i as u8).collect();
        let cs = chunks(VIDEO, true, 7, 123_456, &data);
        assert_eq!(cs.len(), 3);
        for (i, c) in cs.iter().enumerate() {
            assert_eq!(c[0], VIDEO);
            assert_eq!(c[1], 1);
            assert_eq!(u32::from_be_bytes(c[2..6].try_into().unwrap()), 7);
            assert_eq!(u16::from_be_bytes(c[6..8].try_into().unwrap()) as usize, i);
            assert_eq!(u16::from_be_bytes(c[8..10].try_into().unwrap()), 3);
            assert_eq!(u64::from_be_bytes(c[10..18].try_into().unwrap()), 123_456);
            assert!(c.len() <= HEADER + PAYLOAD);
        }
        let joined: Vec<u8> = cs.iter().flat_map(|c| c[HEADER..].to_vec()).collect();
        assert_eq!(joined, data);
    }
}
