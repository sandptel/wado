//! The host's caps, applied to what a client asked for.
//!
//! Clamped, not refused: a phone asking for 120 fps from a host capped at 90 should get a
//! 90 fps session and be told, not an error.

use wado_protocol::{Quality, SessionConfig};

use crate::schema::Stream;

/// Clamp `req` to `limits` in place. Returns one human line per change, for the log and, later,
/// the client's "limited by host" note.
pub fn clamp(req: &mut SessionConfig, limits: &Stream) -> Vec<String> {
    let mut notes = Vec::new();
    if let Some(max) = limits.max_fps {
        if req.fps > max {
            notes.push(format!("fps {} → {max} (host limit)", req.fps));
            req.fps = max;
        }
    }
    // Width and height scale together: clamping one alone would change the aspect, and with it
    // the 1:1 touch mapping (invariant #8). Kept even, as encoders need.
    let fw = limits
        .max_width
        .map_or(1.0, |m| (m as f64 / req.width.max(1) as f64).min(1.0));
    let fh = limits
        .max_height
        .map_or(1.0, |m| (m as f64 / req.height.max(1) as f64).min(1.0));
    let f = fw.min(fh);
    if f < 1.0 {
        let (w, h) = (even(req.width as f64 * f), even(req.height as f64 * f));
        notes.push(format!(
            "{}x{} → {w}x{h} (host limit)",
            req.width, req.height
        ));
        (req.width, req.height) = (w, h);
    }
    if let (Some(max), Quality::Custom { bitrate_kbps }) = (limits.max_bitrate, &mut req.quality) {
        if *bitrate_kbps > max {
            notes.push(format!("bitrate {bitrate_kbps} → {max} kbps (host limit)"));
            *bitrate_kbps = max;
        }
    }
    if let Some(forced) = limits.encoder.forced() {
        if req.encoder.backend != forced {
            notes.push(format!(
                "encoder {:?} → {forced:?} (host setting)",
                req.encoder.backend
            ));
            req.encoder.backend = forced;
        }
    }
    notes
}

fn even(x: f64) -> u32 {
    ((x as u32) & !1).max(16)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::stream::Encoder;

    fn req() -> SessionConfig {
        serde_json::from_str(
            r#"{"width":2400,"height":1080,"fps":120,"quality":{"custom":{"bitrate_kbps":20000}}}"#,
        )
        .unwrap()
    }

    #[test]
    fn caps_apply_and_keep_aspect() {
        let mut r = req();
        let limits = Stream {
            max_fps: Some(90),
            max_width: Some(1920),
            max_bitrate: Some(8000),
            encoder: Encoder::Software,
            ..Default::default()
        };
        let notes = clamp(&mut r, &limits);
        assert_eq!(r.fps, 90);
        assert_eq!((r.width, r.height), (1920, 864));
        assert!(matches!(r.quality, Quality::Custom { bitrate_kbps: 8000 }));
        assert_eq!(r.encoder.backend, wado_protocol::EncoderBackend::Software);
        assert_eq!(notes.len(), 4);
    }

    #[test]
    fn no_limits_no_change() {
        let mut r = req();
        assert!(clamp(&mut r, &Stream::default()).is_empty());
        assert_eq!(r.fps, 120);
    }
}

/// Input settings the host pins (`input { }`) win over the client's.
pub fn pin_input(req: &mut SessionConfig, pinned: &crate::schema::Input) {
    if let Some(v) = pinned.repeat_rate {
        req.input.repeat_rate = v;
    }
    if let Some(v) = pinned.repeat_delay {
        req.input.repeat_delay = v;
    }
    if let Some(v) = pinned.focus_follows_pointer {
        req.input.focus_follows_pointer = v;
    }
}
