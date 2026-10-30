//! What stays off the relay's lane: a copy over [`LANE_MAX`] — a picture, in practice — is not
//! sent; the device is told it stays on the computer ([`RelayMsg::ClipHeld`]), as a file over the
//! lane's limit is (`crate::files::lane`). Every link to a device is the lane today, so the check
//! is unconditional.

use wado_compositor::clipboard::Clip;
use wado_protocol::clip::LANE_MAX;
use wado_protocol::relay::RelayMsg;

/// `None` when `clip` may go; otherwise what to send in its place. `id` names a history entry
/// (an answer to `ClipGet`), `hash` a new copy (in place of `ClipNow`) — one of them empty.
pub fn held(clip: &Clip, id: &str, hash: &str) -> Option<RelayMsg> {
    let n = clip.data.len();
    if n <= LANE_MAX {
        return None;
    }
    let what = if clip.mime.starts_with("image/") {
        "picture"
    } else {
        "copy"
    };
    Some(RelayMsg::ClipHeld {
        id: id.into(),
        hash: hash.into(),
        mime: clip.mime.clone(),
        bytes: n as u64,
        message: format!(
            "This {what} ({:.1} MB) stays on the computer: the relay's limited lane carries \
             clipboard copies up to {} MB. This is the network, not wado.",
            n as f64 / 1_048_576.0,
            LANE_MAX >> 20
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(n: usize) -> Clip {
        Clip {
            mime: "image/png".into(),
            data: vec![0u8; n].into(),
            secret: false,
        }
    }

    #[test]
    fn a_big_picture_stays_and_says_so() {
        let Some(RelayMsg::ClipHeld {
            id, bytes, message, ..
        }) = held(&png(3 << 20), "7", "")
        else {
            panic!("a 3 MB picture went on the lane");
        };
        assert_eq!((id.as_str(), bytes), ("7", 3 << 20));
        assert!(message.contains("picture (3.0 MB)"), "{message}");
    }

    #[test]
    fn small_copies_still_go() {
        assert!(held(&png(LANE_MAX), "7", "").is_none());
        let text = Clip {
            mime: "text/plain".into(),
            data: b"hello".as_slice().into(),
            secret: false,
        };
        assert!(held(&text, "", "h").is_none());
    }
}
