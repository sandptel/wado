//! What each daemon costs this computer: CPU over the last two minutes and memory now, read
//! from `/proc` for every `wado daemon` process.

use std::collections::VecDeque;
use std::time::Instant;

/// Samples kept per daemon — two minutes at one a second.
const KEEP: usize = 120;
/// ponytail: USER_HZ is 100 on every Linux this runs on; read sysconf if one ever differs.
const HZ: f64 = 100.0;

pub struct Daemon {
    pub instance: String,
    /// CPU percent of one core, oldest first.
    pub cpu: VecDeque<f64>,
    /// Resident memory, bytes.
    pub rss: u64,
    ticks: u64,
    at: Instant,
}

#[derive(Default)]
pub struct Load {
    pub daemons: Vec<Daemon>,
}

impl Load {
    pub fn sample(&mut self) {
        let now = Instant::now();
        let seen = scan();
        self.daemons
            .retain(|d| seen.iter().any(|(i, ..)| *i == d.instance));
        for (instance, ticks, rss) in seen {
            match self.daemons.iter_mut().find(|d| d.instance == instance) {
                Some(d) => {
                    let secs = now.duration_since(d.at).as_secs_f64().max(0.001);
                    let pct = ticks.saturating_sub(d.ticks) as f64 / HZ / secs * 100.0;
                    d.cpu.push_back(pct);
                    if d.cpu.len() > KEEP {
                        d.cpu.pop_front();
                    }
                    (d.ticks, d.rss, d.at) = (ticks, rss, now);
                }
                None => self.daemons.push(Daemon {
                    instance,
                    cpu: VecDeque::new(),
                    rss,
                    ticks,
                    at: now,
                }),
            }
        }
        self.daemons
            .sort_by_key(|d| (d.instance.len(), d.instance.clone()));
    }
}

/// `(instance, cpu ticks, rss bytes)` of every `wado daemon` this user can read.
fn scan() -> Vec<(String, u64, u64)> {
    let Ok(rd) = std::fs::read_dir("/proc") else {
        return Vec::new();
    };
    rd.flatten()
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .bytes()
                .all(|b| b.is_ascii_digit())
        })
        .filter_map(|e| {
            let p = e.path();
            let cmd = std::fs::read(p.join("cmdline")).ok()?;
            let mut argv = cmd.split(|b| *b == 0);
            let exe = argv.next()?;
            if !(exe.ends_with(b"/wado") || exe == b"wado") || argv.next()? != b"daemon" {
                return None;
            }
            let env = std::fs::read(p.join("environ")).unwrap_or_default();
            let instance = env
                .split(|b| *b == 0)
                .find_map(|v| v.strip_prefix(b"WADO_INSTANCE="))
                .map_or("1".into(), |v| String::from_utf8_lossy(v).into_owned());
            // Fields after the `(comm)`: utime and stime are the 12th and 13th.
            let stat = std::fs::read_to_string(p.join("stat")).ok()?;
            let f: Vec<&str> = stat.rsplit_once(')')?.1.split_whitespace().collect();
            let ticks = f.get(11)?.parse::<u64>().ok()? + f.get(12)?.parse::<u64>().ok()?;
            let statm = std::fs::read_to_string(p.join("statm")).ok()?;
            let pages: u64 = statm.split_whitespace().nth(1)?.parse().ok()?;
            Some((instance, ticks, pages * 4096))
        })
        .collect()
}
