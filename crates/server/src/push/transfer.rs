//! "Transfer finished": a big upload or download this daemon's device made, read from the
//! files log (`crate::files::audit`) — its own lines only, so the pool says it once.

use std::io::{Read, Seek, SeekFrom};
use std::time::Duration;

use wado_protocol::push::{NOTE_TRANSFER, PushNote};

/// Below this a transfer is over before anyone looks away.
pub const BIG: u64 = 20 << 20;

/// `(device name, verb, path, size)` for a finished big transfer in one log line, if it is one
/// of `me`'s.
pub fn finished(line: &str, me: &str) -> Option<(String, &'static str, String, u64)> {
    let f: Vec<&str> = line.split('\t').collect();
    if f.len() < 7 || f[1] != me || f[6] != "ok" {
        return None;
    }
    let verb = match f[3] {
        "upload" => "Sent to",
        "download" | "download-zip" => "Saved from",
        _ => return None,
    };
    let size: u64 = f[5].parse().ok()?;
    (size >= BIG).then(|| (f[2].to_string(), verb, f[4].to_string(), size))
}

fn mb(n: u64) -> String {
    if n >= 1 << 30 {
        format!("{:.1} GB", n as f64 / (1u64 << 30) as f64)
    } else {
        format!("{} MB", n >> 20)
    }
}

/// Follow the log for the daemon's lifetime.
pub async fn follow() {
    let p = crate::files::audit::log_path();
    let me = wado_config::live::current().server.instance.clone();
    let mut pos = std::fs::metadata(&p).map_or(0, |m| m.len());
    loop {
        tokio::time::sleep(Duration::from_secs(2)).await;
        let Ok(mut f) = std::fs::File::open(&p) else {
            continue;
        };
        let len = f.metadata().map_or(0, |m| m.len());
        if len < pos {
            pos = 0;
        }
        if len == pos || f.seek(SeekFrom::Start(pos)).is_err() {
            continue;
        }
        let mut text = String::new();
        if f.read_to_string(&mut text).is_err() {
            continue;
        }
        let upto = text.rfind('\n').map_or(0, |i| i + 1);
        pos += upto as u64;
        for line in text[..upto].lines() {
            let Some((device, verb, path, size)) = finished(line, &me) else {
                continue;
            };
            let Ok(d) = crate::gate::Gate::default().find(&device) else {
                continue;
            };
            let (computer, name) = super::computer();
            let file = path.rsplit('/').next().unwrap_or(&path).to_string();
            let key = d.key.clone();
            super::deliver(
                PushNote {
                    kind: NOTE_TRANSFER.into(),
                    title: format!("{verb} {name} · {}", mb(size)),
                    body: file,
                    computer,
                    computer_name: name,
                    tag: "transfer".into(),
                    at: crate::passkey::now_s(),
                    ..Default::default()
                },
                move |e| e.device == key && e.kinds.transfer,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_big_finished_transfers_of_this_daemon() {
        let line = |inst: &str, op: &str, size: u64, res: &str| {
            format!("1\t{inst}\tPhone\t{op}\t/home/me/v.mkv\t{size}\t{res}")
        };
        let big = BIG + 1;
        assert_eq!(
            finished(&line("1", "upload", big, "ok"), "1"),
            Some(("Phone".into(), "Sent to", "/home/me/v.mkv".into(), big))
        );
        assert!(finished(&line("1", "download-zip", big, "ok"), "1").is_some());
        assert!(
            finished(&line("2", "upload", big, "ok"), "1").is_none(),
            "another daemon's"
        );
        assert!(
            finished(&line("1", "upload", 10, "ok"), "1").is_none(),
            "small"
        );
        assert!(
            finished(&line("1", "upload", big, "err: x"), "1").is_none(),
            "failed"
        );
        assert!(finished(&line("1", "rename", big, "ok"), "1").is_none());
        assert_eq!(mb(25 << 20), "25 MB");
    }
}
