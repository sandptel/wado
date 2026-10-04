//! The download rate cap while a session's video is live (Decision Log `2026-10-04`, item 8).
//!
//! The files peer connection has its own SCTP association, so it cannot head-of-line block
//! input; but it shares the path, and a download at full tilt can still crowd out the video.
//! So while video is live, downloads are held to `files.rate-with-video` — halved for a few
//! seconds after the viewer reports loss (RTCP) or a strained decoder.
//!
//! ponytail: one process-wide token bucket; per-transfer fairness is the upgrade path.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

static VIDEO: AtomicBool = AtomicBool::new(false);
static STRAINED: AtomicBool = AtomicBool::new(false);
/// Unix ms until which the cap stays halved after reported loss.
static LOSS_UNTIL: AtomicU64 = AtomicU64::new(0);
static BUCKET: Mutex<Option<(Instant, f64)>> = Mutex::new(None);

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

pub fn video(live: bool) {
    VIDEO.store(live, Ordering::Relaxed);
}
pub fn strained(s: bool) {
    STRAINED.store(s, Ordering::Relaxed);
}
/// The viewer's receiver report said `fraction` (of 256) of video packets were lost.
pub fn loss(fraction: u8) {
    if fraction > 5 {
        LOSS_UNTIL.store(now_ms() + 5_000, Ordering::Relaxed);
    }
}

/// Bytes per second allowed now, or `None` for no cap.
pub fn cap() -> Option<f64> {
    let mbps = wado_config::live::current().files.rate_with_video;
    if !VIDEO.load(Ordering::Relaxed) || mbps == 0 {
        return None;
    }
    let mut bps = f64::from(mbps) * 125_000.0;
    if STRAINED.load(Ordering::Relaxed) || LOSS_UNTIL.load(Ordering::Relaxed) > now_ms() {
        bps /= 2.0;
    }
    Some(bps)
}

/// Wait until `bytes` may be sent.
pub async fn take(bytes: usize) {
    let Some(bps) = cap() else { return };
    let wait = {
        let mut b = BUCKET.lock().unwrap_or_else(|e| e.into_inner());
        let now = Instant::now();
        let (last, tokens) = b.unwrap_or((now, bps / 4.0));
        // Refill, at most a quarter second of burst.
        let tokens =
            (tokens + now.duration_since(last).as_secs_f64() * bps).min(bps / 4.0) - bytes as f64;
        *b = Some((now, tokens));
        (tokens < 0.0).then(|| Duration::from_secs_f64(-tokens / bps))
    };
    if let Some(w) = wait {
        tokio::time::sleep(w).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capped_only_while_video_is_live_and_halved_on_loss() {
        video(false);
        assert_eq!(cap(), None, "no video: no cap");
        video(true);
        let full = cap().expect("video live: capped");
        assert_eq!(full, 40.0 * 125_000.0, "the default 40 Mbps");
        loss(3);
        assert_eq!(cap(), Some(full), "a little loss is noise");
        loss(40);
        assert_eq!(cap(), Some(full / 2.0), "real loss halves it");
        LOSS_UNTIL.store(0, Ordering::Relaxed);
        strained(true);
        assert_eq!(cap(), Some(full / 2.0), "a strained decoder halves it");
        strained(false);
        video(false);
    }
}
