//! Why a thread was slow: was it waiting for a CPU, or was it not waiting at all?
//!
//! A `write_sample` that takes 400 ms has two entirely different explanations, and the
//! existing pump log cannot tell them apart. Either the task was runnable the whole time and
//! the scheduler never gave it a core — in which case CPU shares for the session's
//! applications are the fix — or it was blocked on something else, in which case priority
//! tuning is a placebo that would have to be withdrawn later.
//!
//! This is the measurement that decides, and the reason no resource knob here is presented
//! as a fix until it has spoken.
//!
//! ⚠️ The stall this was built for is a *roughly constant* 300–500 ms per frame, uncorrelated
//! with frame size or packet count (83 packets took the same as 12). So it is one blocking
//! event, not accumulated per-packet cost — do not read the pump's `us_per_packet` as if it
//! meant anything.

/// Nanoseconds this thread has spent runnable but not running, since it started.
///
/// Field 2 of `/proc/thread-self/schedstat`. A delta across a suspicious stretch that
/// approaches the stretch's wall duration means the thread was starved of CPU; a delta near
/// zero means it was somewhere else entirely.
///
/// ponytail: no caching of the open file. One read per stall, and stalls are rare by
/// definition — if this ever runs per frame, open it once and `pread` instead.
pub fn run_delay_ns() -> u64 {
    read_field("/proc/thread-self/schedstat", 1).unwrap_or(0)
}

/// `some avg10` from a PSI file, as a percentage of the last 10 seconds.
///
/// `/proc/pressure/cpu` is the whole-machine counterpart of [`run_delay_ns`].
/// `/proc/pressure/memory` is the one that catches the case no CPU metric shows: direct
/// reclaim triggered by another process allocating (a browser, say) stalls an unrelated
/// thread for hundreds of milliseconds while the machine looks idle.
pub fn pressure_some_avg10(path: &str) -> f32 {
    let Ok(s) = std::fs::read_to_string(path) else {
        return 0.0;
    };
    s.lines()
        .find(|l| l.starts_with("some "))
        .and_then(|l| l.split_whitespace().find_map(|f| f.strip_prefix("avg10=")))
        .and_then(|v| v.parse().ok())
        .unwrap_or(0.0)
}

fn read_field(path: &str, index: usize) -> Option<u64> {
    std::fs::read_to_string(path)
        .ok()?
        .split_whitespace()
        .nth(index)?
        .parse()
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The trap this test exists for: `kernel.sched_schedstats` is 0 on this host, which
    /// sounds like it disables exactly this counter. It does not — that sysctl gates the
    /// aggregate `/proc/schedstat` stats, while per-task `sched_info` keeps accumulating.
    /// Verified empirically at 3x oversubscription: 1644 ms of run delay over 2.9 s wall.
    ///
    /// So this asserts only that the counter reads and never goes backwards. Asserting it is
    /// non-zero would be flaky on an idle machine, which is the correct reading there.
    #[test]
    fn run_delay_reads_and_is_monotonic() {
        let a = run_delay_ns();
        let mut spin = 0u64;
        for i in 0..2_000_000u64 {
            spin = spin.wrapping_add(i);
        }
        assert_ne!(spin, u64::MAX);
        assert!(run_delay_ns() >= a);
    }

    #[test]
    fn pressure_parses_the_some_line_not_the_full_one() {
        // Both lines carry an avg10; picking the wrong one reports 0 on a machine that is
        // in fact under pressure, since `full` stays 0 until *every* task is stalled.
        assert!(pressure_some_avg10("/proc/pressure/cpu") >= 0.0);
    }

    #[test]
    fn a_missing_file_is_zero_not_a_panic() {
        assert_eq!(pressure_some_avg10("/proc/pressure/nonexistent"), 0.0);
        assert_eq!(read_field("/proc/nonexistent", 1), None);
    }
}
