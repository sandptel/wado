//! `wado passkey` — which devices have a passkey here, and `remove` for one that lost its own
//! (a new phone, a wiped browser): the device then adds a new one from Connection & trust.
//! Removing is the computer's call alone; a device cannot replace its passkey without it.

use super::style::{accent, bold, dim, good};
use crate::gate::Gate;
use crate::passkey::Store;

pub fn run(args: &[String]) -> i32 {
    match args.first().map(String::as_str) {
        None | Some("list") => list(),
        Some("remove" | "reset") => remove(args.get(1).map(String::as_str)),
        Some(other) => {
            eprintln!(
                "  usage: wado passkey [list] | wado passkey remove <device>   (not `{other}`)"
            );
            2
        }
    }
}

fn list() -> i32 {
    let gate = Gate::default();
    let store = Store::default();
    let devices = gate.devices();
    if devices.is_empty() {
        println!("  no trusted devices yet — {}", accent("wado pair"));
        return 0;
    }
    println!();
    for d in devices {
        match store.get(&d.key) {
            Some(c) => println!(
                "  {} {}  {}",
                good("●"),
                d.name,
                dim(&format!("passkey for {}", c.origin))
            ),
            None => println!(
                "  {} {}  {}",
                dim("○"),
                d.name,
                dim("no passkey — no shells, file writes or trackpad until it adds one")
            ),
        }
    }
    println!();
    0
}

fn remove(who: Option<&str>) -> i32 {
    let Some(who) = who else {
        eprintln!("  usage: wado passkey remove <device>   — its name, or the start of its key");
        return 2;
    };
    let gate = Gate::default();
    let d = match gate.find(who) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("  {e}");
            return 1;
        }
    };
    if Store::default().remove(&d.key) {
        println!(
            "  removed {}'s passkey — it can add a new one from the computer's page → Connection & trust",
            bold(&d.name)
        );
        0
    } else {
        println!("  {} has no passkey here", bold(&d.name));
        0
    }
}
