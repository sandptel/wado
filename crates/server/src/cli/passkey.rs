//! `wado passkey` — which devices have a passkey here and how long their unlock lasts; `lock`
//! for one (or all) of them now; and `remove` for one that lost its own (a new phone, a wiped
//! browser): the device then adds a new one from Connection & trust. Removing is the
//! computer's call alone; a device cannot replace its passkey without it.
//!
//! The two ways out when a passkey will not work (Decision Log `2026-10-30`): `approve` answers a
//! phone that asked to be let in without it — the person at this computer says yes or no — and
//! `require <device> on|off` decides whether the computer asks that device for one at all.

use std::io::Write as _;
use std::os::unix::net::UnixStream;

use super::style::{accent, bold, dim, good, warn};
use crate::gate::Gate;
use crate::passkey::Store;

pub fn run(args: &[String]) -> i32 {
    match args.first().map(String::as_str) {
        None | Some("list") => list(),
        Some("remove" | "reset") => remove(args.get(1).map(String::as_str)),
        Some("lock") => lock(args.get(1).map(String::as_str)),
        Some("approve") => super::passkey_approve::run(&args[1..]),
        Some("require") => require(
            args.get(1).map(String::as_str),
            args.get(2).map(String::as_str),
        ),
        Some(other) => {
            eprintln!("  {USAGE}   (not `{other}`)");
            2
        }
    }
}

const USAGE: &str = "usage: wado passkey [list] | lock <device|all> | remove <device> | \
    approve [<device>] [--yes|--no] [--wait] | require <device> on|off";

/// How long `stay` keeps a device unlocked, in the card's words.
fn lasts(stay: &str) -> &'static str {
    match stay {
        "1h" => "stays unlocked 1 hour idle",
        "8h" => "stays unlocked 8 hours idle",
        "lock" => "stays unlocked until it locks itself",
        _ => "locks after 15 min idle",
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
        if !store.required(&d.key) {
            println!(
                "  {} {}  {}",
                warn("◌"),
                d.name,
                dim("passkey not asked — shells, file writes and the trackpad open without one")
            );
            continue;
        }
        match store.get(&d.key) {
            Some(c) => println!(
                "  {} {}  {}",
                good("●"),
                d.name,
                dim(&format!(
                    "passkey for {} · {}",
                    c.origin,
                    lasts(&store.prefs(&d.key).stay)
                ))
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

/// Tell every running daemon to shut `key`'s unlock now (`None`: everyone's). The mark on file
/// is cleared here too, so one that is not running opens nothing when it starts.
pub fn lock_everywhere(key: Option<&str>) {
    let store = Store::default();
    match key {
        Some(k) => crate::passkey::unlock::lock_in(&store, k),
        None => {
            for p in store.all_prefs() {
                crate::passkey::unlock::lock_in(&store, &p.key);
            }
        }
    }
    let line = format!("passkey-lock {}\n", key.unwrap_or("all"));
    for inst in &super::status::daemons() {
        if let Ok(mut s) = UnixStream::connect(crate::config::socket::path_for(inst)) {
            // Wait for the answer, so the lock has happened when this returns.
            let mut answer = String::new();
            let _ = s
                .write_all(line.as_bytes())
                .and_then(|()| std::io::Read::read_to_string(&mut s, &mut answer));
        }
    }
}

fn lock(who: Option<&str>) -> i32 {
    let Some(who) = who else {
        eprintln!("  usage: wado passkey lock <device|all>   — its name, or the start of its key");
        return 2;
    };
    if who == "all" {
        lock_everywhere(None);
        println!("  locked every device — each asks for its passkey again");
        return 0;
    }
    match Gate::default().find(who) {
        Ok(d) => {
            lock_everywhere(Some(&d.key));
            println!("  locked {} — it asks for its passkey again", bold(&d.name));
            0
        }
        Err(e) => {
            eprintln!("  {e}");
            1
        }
    }
}

/// Ask `who` for its passkey (`on`, the default), or stop asking it.
fn require(who: Option<&str>, on: Option<&str>) -> i32 {
    let (Some(who), Some(on @ ("on" | "off"))) = (who, on) else {
        eprintln!(
            "  usage: wado passkey require <device> on|off   — its name, or the start of its key"
        );
        return 2;
    };
    let d = match Gate::default().find(who) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("  {e}");
            return 1;
        }
    };
    let store = Store::default();
    if let Err(e) = store.set_required(&d.key, on == "on") {
        eprintln!("  could not save that: {e}");
        return 1;
    }
    if on == "on" {
        // Asked again from now: any open window shuts, everywhere.
        lock_everywhere(Some(&d.key));
        println!(
            "  {} is asked for its passkey again — shells, file writes and the trackpad need it",
            bold(&d.name)
        );
    } else {
        println!(
            "  {} is no longer asked for a passkey — anyone holding it can open shells, change files \
             and use the trackpad. Turn it back on with {}",
            bold(&d.name),
            accent(&format!("wado passkey require {} on", Gate::typed(&d.name)))
        );
    }
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
        lock_everywhere(Some(&d.key));
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
