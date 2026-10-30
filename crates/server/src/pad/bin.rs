//! Compact touch frames in (`wado_protocol::pad_bin`): a binary message on the direct link's
//! `pad-touch` channel, or base64 in a relayed [`PadMsg::Bin`], turned back into the
//! [`PadMsg::Frame`] the pad understands. Anything that is not exactly one frame is `None`.

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use wado_protocol::pad::PadMsg;
use wado_protocol::pad_bin;

/// A frame from its bytes.
pub fn from_bytes(b: &[u8]) -> Option<PadMsg> {
    pad_bin::decode(b).map(|(seq, t, c)| PadMsg::Frame { seq, t, c })
}

/// A relayed compact frame unpacked; any other message as it is.
pub fn unpack(msg: PadMsg) -> Option<PadMsg> {
    match msg {
        PadMsg::Bin { b } => STANDARD.decode(b).ok().and_then(|b| from_bytes(&b)),
        other => Some(other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_relayed_compact_frame_is_the_frame_it_encodes() {
        let bytes = pad_bin::encode(9, 1234, &[(0, 1, 2), (3, 65535, 0)]);
        let b = STANDARD.encode(&bytes);
        assert_eq!(
            unpack(PadMsg::Bin { b }),
            Some(PadMsg::Frame {
                seq: 9,
                t: 1234,
                c: vec![(0, 1, 2), (3, 65535, 0)]
            })
        );
        // The protocol test's literal, as the browser's `btoa` writes it.
        assert_eq!(
            unpack(PadMsg::Bin {
                b: "AAAABwAAA+gAAGT//w==".into()
            }),
            Some(PadMsg::Frame {
                seq: 7,
                t: 1000,
                c: vec![(0, 100, 65535)]
            })
        );
        assert_eq!(unpack(PadMsg::Bin { b: "!!".into() }), None);
        assert_eq!(unpack(PadMsg::Bye), Some(PadMsg::Bye));
    }
}
