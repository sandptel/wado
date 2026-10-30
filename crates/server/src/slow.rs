//! Where a request's time went — said only when it is too long.
//!
//! A device gives up with "the computer didn't answer" after a few seconds, and nothing on the
//! daemon side used to say which stage spent them. Each stage that can hold an answer back is
//! timed: the relay loop (one frame's handler holds every frame behind it — a [`Span`]), and the
//! lane (how long a frame sat queued, how long the socket write took — [`report`]). Past [`SLOW`]
//! it is a `WARN` starting `slow:`, so one grep ranks the causes; past [`NOTABLE`] a `debug`
//! starting `timing:`. Below that, nothing — the cost is one `Instant::now()` per frame.

use std::time::{Duration, Instant};

use tracing::{debug, warn};

/// A stage over this is a person waiting: logged at `WARN` with a `slow:` prefix.
pub const SLOW: Duration = Duration::from_millis(250);

/// Over this, worth a `debug` line: enough to see a stage creeping up before it is slow.
pub const NOTABLE: Duration = Duration::from_millis(50);

/// Log a measured stage if it was long. `kind` is the frame's `type`, `seq` its place on the
/// link, `detail` whatever else locates it (queue depth, socket time).
pub fn report(stage: &str, took: Duration, kind: &str, seq: u64, detail: std::fmt::Arguments<'_>) {
    let ms = took.as_millis();
    if took >= SLOW {
        warn!("slow: {stage} {kind} #{seq} took {ms} ms {detail}");
    } else if took >= NOTABLE {
        debug!("timing: {stage} {kind} #{seq} took {ms} ms {detail}");
    }
}

/// One frame's stage, reported when dropped — so an arm that `continue`s or `break`s early is
/// still timed.
pub struct Span {
    stage: &'static str,
    kind: String,
    seq: u64,
    t0: Instant,
}

impl Span {
    pub fn new(stage: &'static str, kind: &str, seq: u64) -> Self {
        Self {
            stage,
            kind: kind.to_owned(),
            seq,
            t0: Instant::now(),
        }
    }
}

impl Drop for Span {
    fn drop(&mut self) {
        report(
            self.stage,
            self.t0.elapsed(),
            &self.kind,
            self.seq,
            format_args!(""),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thresholds_are_ordered() {
        assert!(NOTABLE < SLOW);
    }

    #[test]
    fn a_span_reports_on_every_exit_path() {
        // Drop runs on an early return as on a fall-through; this only checks it does not panic.
        for early in [true, false] {
            let _s = Span::new("test", "x", 1);
            if early {
                continue;
            }
        }
    }
}
