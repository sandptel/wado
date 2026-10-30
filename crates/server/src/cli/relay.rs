//! `wado relay [status] [--json]` — is wado's relay up, and does each of this computer's daemons
//! have it? Two views that disagree in a useful way: the relay's `/health`, asked from here
//! right now, and each running daemon's own record of its link (`crate::relay_watch`).
//!
//! Exit status: 0 when the relay answers and every running daemon is registered with it, 1
//! otherwise — so a cron line or a systemd timer can watch it.

use serde_json::{Value, json};

use super::status::{self, host_of};
use super::style::{accent, bad, bold, dim, dot, warn};
use crate::relay_watch::record::{self, Record, span};

pub fn run(args: &[String]) -> i32 {
    let mut json_out = false;
    for a in args {
        match a.as_str() {
            "status" => {}
            "--json" => json_out = true,
            "-h" | "--help" | "help" => {
                println!(
                    "  {} — is the relay up, and does each daemon here have it?",
                    accent("wado relay status [--json]")
                );
                println!(
                    "  {}",
                    dim("exits 1 when the relay is unreachable or a daemon has lost it")
                );
                return 0;
            }
            other => {
                eprintln!("  {} is not a relay command", bad(other));
                return 2;
            }
        }
    }
    let url = status::relay_url();
    let health = probe(&url);
    let links = record::list();
    let ok = health.is_some() && links.iter().all(|l| l.up);
    if json_out {
        let out = json!({
            "url": url,
            "reachable": health.is_some(),
            "health": health,
            "daemons": links,
        });
        println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
    } else {
        print(&url, health.as_ref(), &links);
    }
    if ok { 0 } else { 1 }
}

/// The relay's `/health`, from this computer: over TLS for a `wss://` relay.
fn probe(url: &str) -> Option<Value> {
    let addr = host_of(url);
    let body = if url.starts_with("wss://") {
        status::https_get(&addr, "/health")
    } else {
        status::health(&addr)
    }?;
    serde_json::from_str(&body).ok()
}

fn print(url: &str, health: Option<&Value>, links: &[Record]) {
    let host = host_of(url);
    println!();
    match health {
        Some(h) => {
            let up = h["uptime_s"]
                .as_u64()
                .map_or(String::new(), |s| format!(" · up {}", span(s * 1000)));
            let ver = h["version"]
                .as_str()
                .map_or(String::new(), |v| format!(" · v{v}"));
            let lane = h["lane"]["bytes_per_sec"].as_u64().map_or_else(
                || " · no lane cap (an older relay)".to_string(),
                |b| format!(" · lane {} KB/s", b / 1024),
            );
            println!(
                "  {} {} {host}{}",
                dot(true),
                bold("relay"),
                dim(&format!("{up}{ver}{lane}"))
            );
        }
        None => println!(
            "  {} {} {host} {}",
            dot(false),
            bold("relay"),
            bad("— unreachable from this computer")
        ),
    }
    if links.is_empty() {
        println!(
            "  {}",
            dim("no daemon running here — start one with `wado daemon`")
        );
    }
    let now = record::now_ms();
    for l in links {
        let since = span(l.for_ms(now));
        let history = if l.outages == 0 {
            String::new()
        } else {
            format!(
                " · {} outage{} since start, longest {}",
                l.outages,
                if l.outages == 1 { "" } else { "s" },
                span(l.longest_ms)
            )
        };
        if l.up {
            println!(
                "  {} {} connected for {since}{}",
                dot(true),
                bold(&format!("daemon {}", l.instance)),
                dim(&history)
            );
        } else {
            let why = if l.error.is_empty() {
                String::new()
            } else {
                format!(" ({})", l.error)
            };
            println!(
                "  {} {} {}{}",
                dot(false),
                bold(&format!("daemon {}", l.instance)),
                warn(&format!("without the relay for {since} — retrying")),
                dim(&format!("{why}{history}"))
            );
        }
    }
    if health.is_none() || links.iter().any(|l| !l.up) {
        println!();
        println!(
            "  {}",
            dim("wado's relay is unreachable — not your computer. Phones cannot reach it until")
        );
        println!(
            "  {}",
            dim("the relay answers; the daemons keep retrying. Is this computer online?")
        );
    }
    println!();
}
