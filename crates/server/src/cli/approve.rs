//! `wado approve [once]` — let in the device waiting at this computer's gate, from the computer
//! itself. For when no other device is connected to approve it (a new phone, a cleared browser).

use crate::gate::{Gate, Verdict};

pub fn run(args: &[String]) -> i32 {
    let gate = Gate::default();
    let pending = gate.pending();
    let Some(req) = pending.last() else {
        println!("no device is waiting — connect it first, then run this while it waits");
        return 1;
    };
    let verdict = if args.first().map(String::as_str) == Some("once") {
        Verdict::Once
    } else {
        Verdict::Always
    };
    if !gate.answer(&req.id, verdict) {
        eprintln!("could not answer the request");
        return 1;
    }
    let how = if verdict == Verdict::Always {
        "trusted from now on"
    } else {
        "let in this once"
    };
    println!("approved {} ({}) — {how}", req.name, req.addr);
    for other in &pending[..pending.len() - 1] {
        println!(
            "  also waiting: {} ({}) — run again to approve it",
            other.name, other.addr
        );
    }
    0
}
