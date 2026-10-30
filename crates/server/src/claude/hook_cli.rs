//! `wado claude-event` — what Claude Code's hooks run (`wado claude hooks install`). Reads the
//! hook's JSON on stdin, hands it to every daemon on this computer, and for a permission request
//! waits for the first answer from a phone and prints it as Claude's decision.
//!
//! It never fails Claude: with no daemon, bad input, or no answer it prints nothing and exits 0,
//! which leaves Claude to do what it would have done without wado.

use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::UnixStream;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use super::event::Envelope;

/// Claude's input is a few KB; anything past this is not a hook event.
const MAX_INPUT: u64 = 1 << 20;

/// The tmux session this hook runs in, if any — asked of tmux by the pane Claude is in.
fn tmux_session() -> String {
    let (Some(_), Some(pane)) = (std::env::var_os("TMUX"), std::env::var_os("TMUX_PANE")) else {
        return String::new();
    };
    std::process::Command::new("tmux")
        .arg("display-message")
        .arg("-p")
        .arg("-t")
        .arg(pane)
        .arg("#S")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}

fn sockets() -> Vec<std::path::PathBuf> {
    crate::cli::status::daemons()
        .iter()
        .map(|i| crate::config::socket::path_for(i))
        .collect()
}

/// One request to one daemon; its one-line answer.
fn ask(path: &std::path::Path, line: &str, wait: Duration) -> Option<String> {
    let mut s = UnixStream::connect(path).ok()?;
    s.set_read_timeout(Some(wait)).ok()?;
    s.write_all(format!("{line}\n").as_bytes()).ok()?;
    let mut answer = String::new();
    BufReader::new(s).read_line(&mut answer).ok()?;
    Some(answer.trim().to_string())
}

/// What Claude reads back for a decision (Claude Code's `PermissionRequest` hook output).
pub fn decision(allow: bool) -> String {
    let d = if allow {
        serde_json::json!({ "behavior": "allow" })
    } else {
        serde_json::json!({ "behavior": "deny", "message": "Denied from your phone (wado)." })
    };
    serde_json::json!({
        "hookSpecificOutput": { "hookEventName": "PermissionRequest", "decision": d }
    })
    .to_string()
}

pub fn run() -> i32 {
    let mut input = String::new();
    if std::io::stdin()
        .take(MAX_INPUT)
        .read_to_string(&mut input)
        .is_err()
    {
        return 0;
    }
    let Ok(event) = serde_json::from_str::<serde_json::Value>(&input) else {
        return 0;
    };
    let permission =
        event.get("hook_event_name").and_then(|v| v.as_str()) == Some("PermissionRequest");
    let ask_id = if permission {
        format!("{:016x}", rand::random::<u64>())
    } else {
        String::new()
    };
    let env = Envelope {
        ask: ask_id.clone(),
        tmux: tmux_session(),
        label: std::env::var("WADO_CLAUDE").unwrap_or_default(),
        event,
    };
    let Ok(json) = serde_json::to_string(&env) else {
        return 0;
    };
    let line = format!("claude {json}");
    let daemons = sockets();
    if !permission {
        for d in &daemons {
            ask(d, &line, Duration::from_secs(2));
        }
        return 0;
    }
    // Every daemon at once; the first real answer wins.
    let (tx, rx) = mpsc::channel();
    for d in daemons.clone() {
        let (tx, line) = (tx.clone(), line.clone());
        std::thread::spawn(move || {
            let a = ask(&d, &line, super::socket::WAIT + Duration::from_secs(20));
            let _ = tx.send(a.unwrap_or_default());
        });
    }
    drop(tx);
    let deadline = Instant::now() + super::socket::WAIT + Duration::from_secs(25);
    let mut verdict = None;
    while let Some(left) = deadline.checked_duration_since(Instant::now()) {
        match rx.recv_timeout(left) {
            Ok(a) if a == "allow" => verdict = Some(true),
            Ok(a) if a == "deny" => verdict = Some(false),
            Ok(_) => continue,
            Err(_) => break,
        }
        break;
    }
    // Clear the card everywhere else.
    for d in &daemons {
        ask(
            d,
            &format!("claude-resolved {ask_id}"),
            Duration::from_secs(2),
        );
    }
    if let Some(allow) = verdict {
        println!("{}", decision(allow));
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_decision_is_what_claude_reads() {
        let v: serde_json::Value = serde_json::from_str(&decision(true)).unwrap();
        assert_eq!(
            v["hookSpecificOutput"]["hookEventName"],
            "PermissionRequest"
        );
        assert_eq!(v["hookSpecificOutput"]["decision"]["behavior"], "allow");
        let v: serde_json::Value = serde_json::from_str(&decision(false)).unwrap();
        assert_eq!(v["hookSpecificOutput"]["decision"]["behavior"], "deny");
    }
}
