//! How evenly touch frames arrive, so whether the network really jitters is measured rather
//! than assumed: the gaps between frames while a finger is down, and how many came too late for
//! the pacer to place them. Summed up every [`REPORT_MS`] while the pad is in use.

/// How often the summary is written.
pub const REPORT_MS: u64 = 5000;

#[derive(Default)]
pub struct Arrivals {
    /// Ms between consecutive frames, while a finger was down, since the last report.
    gaps: Vec<u64>,
    /// The last frame's arrival, and whether it held a finger.
    last: Option<(u64, bool)>,
    late: u32,
    since: Option<u64>,
}

impl Arrivals {
    /// A frame arrived at `now_ms`; `down` when it holds a finger, `late` when the pacer had
    /// already played past it.
    pub fn record(&mut self, now_ms: u64, down: bool, late: bool) {
        if let Some((at, was_down)) = self.last
            && (was_down || down)
        {
            self.gaps.push(now_ms.saturating_sub(at));
        }
        self.last = Some((now_ms, down));
        self.late += u32::from(late);
        self.since.get_or_insert(now_ms);
    }

    /// The summary, once [`REPORT_MS`] has passed since the first frame it covers; then starts
    /// over. Nothing while no finger has been down.
    pub fn report(&mut self, now_ms: u64) -> Option<String> {
        let since = self.since?;
        if now_ms.saturating_sub(since) < REPORT_MS {
            return None;
        }
        self.since = None;
        let late = std::mem::take(&mut self.late);
        let mut gaps = std::mem::take(&mut self.gaps);
        if gaps.is_empty() {
            return None;
        }
        gaps.sort_unstable();
        let rank = |p: usize| gaps[(gaps.len() * p).div_ceil(100).max(1) - 1];
        Some(format!(
            "pad: arrivals n={} gap p50={} p90={} max={} late={late}",
            gaps.len(),
            rank(50),
            rank(90),
            gaps[gaps.len() - 1],
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gaps_are_summed_up_every_five_seconds() {
        let mut a = Arrivals::default();
        let mut now = 1000;
        for (i, gap) in [16, 16, 16, 16, 16, 16, 16, 16, 40, 100]
            .into_iter()
            .enumerate()
        {
            a.record(now, true, i == 3);
            now += gap;
        }
        a.record(now, false, false); // the lift ends the run: its gap counts
        assert_eq!(a.report(now), None, "not five seconds yet");
        assert_eq!(
            a.report(6000).as_deref(),
            Some("pad: arrivals n=10 gap p50=16 p90=40 max=100 late=1")
        );
        assert_eq!(a.report(12_000), None, "started over");
    }

    #[test]
    fn gaps_with_no_finger_down_are_not_counted() {
        let mut a = Arrivals::default();
        a.record(0, false, false);
        a.record(500, false, false);
        assert_eq!(a.report(6000), None);
    }
}
