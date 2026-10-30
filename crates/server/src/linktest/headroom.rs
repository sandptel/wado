//! What this computer has to spare, for the speed test's verdict: free memory, CPU load, the
//! encoder in use, sessions running against the most it runs, and the best download any device
//! has measured from it lately (a floor on its upload — nothing here can measure that directly).

use std::sync::Mutex;
use std::time::{Duration, Instant};

use wado_protocol::multi::HostHeadroom;

/// A device's measurement counts towards the upload estimate this long.
const UPLINK_MEMORY: Duration = Duration::from_secs(15 * 60);

fn best() -> &'static Mutex<Vec<(Instant, u32)>> {
    static B: std::sync::OnceLock<Mutex<Vec<(Instant, u32)>>> = std::sync::OnceLock::new();
    B.get_or_init(Default::default)
}

/// A device pulled `kbps` from this computer.
pub fn measured(kbps: u32) {
    let mut b = best().lock().unwrap_or_else(|e| e.into_inner());
    b.retain(|(t, _)| t.elapsed() < UPLINK_MEMORY);
    b.push((Instant::now(), kbps));
}

/// The best recent measurement, kbps; 0 when there is none.
pub fn uplink_kbps() -> u32 {
    let b = best().lock().unwrap_or_else(|e| e.into_inner());
    b.iter()
        .filter(|(t, _)| t.elapsed() < UPLINK_MEMORY)
        .map(|(_, k)| *k)
        .max()
        .unwrap_or(0)
}

/// `MemAvailable` from `/proc/meminfo`, in MB.
pub fn mem_avail_mb(meminfo: &str) -> u32 {
    meminfo
        .lines()
        .find_map(|l| l.strip_prefix("MemAvailable:"))
        .and_then(|v| v.split_whitespace().next()?.parse::<u64>().ok())
        .map_or(0, |kb| (kb / 1024) as u32)
}

/// The 1-minute load average per core, from `/proc/loadavg`.
pub fn load_per_core(loadavg: &str, cores: usize) -> f32 {
    loadavg
        .split_whitespace()
        .next()
        .and_then(|v| v.parse::<f32>().ok())
        .map_or(0.0, |l| l / cores.max(1) as f32)
}

/// Read it all now. `encoder` is the running session's (`hardware`/`software`), if any.
pub fn read(encoder: &str) -> HostHeadroom {
    let cores = std::thread::available_parallelism().map_or(1, |n| n.get());
    let sessions = crate::sessions::list().len() as u32;
    HostHeadroom {
        mem_avail_mb: mem_avail_mb(&std::fs::read_to_string("/proc/meminfo").unwrap_or_default()),
        load_per_core: load_per_core(
            &std::fs::read_to_string("/proc/loadavg").unwrap_or_default(),
            cores,
        ),
        encoder: encoder.to_string(),
        sessions,
        max_sessions: wado_config::live::current().server.max_sessions,
        uplink_kbps: uplink_kbps(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proc_files_are_read() {
        let mi = "MemTotal:       32000000 kB\nMemFree:  100 kB\nMemAvailable:   2048000 kB\n";
        assert_eq!(mem_avail_mb(mi), 2000);
        assert_eq!(mem_avail_mb(""), 0);
        assert!((load_per_core("4.00 3.0 2.0 1/200 99", 8) - 0.5).abs() < 1e-6);
        assert_eq!(load_per_core("", 8), 0.0);
    }

    #[test]
    fn the_best_recent_measurement_is_the_uplink() {
        measured(3000);
        measured(9000);
        measured(4000);
        assert!(uplink_kbps() >= 9000);
    }
}
