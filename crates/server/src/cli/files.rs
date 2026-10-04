//! `wado files` — which devices may use the file manager, from the computer itself.
//!
//! ```text
//!   wado files                     list the trusted devices and their access
//!   wado files grant <device> rw   <device>: its name, or the start of its key; ro | rw | none
//! ```

use crate::gate::Gate;

pub fn run(args: &[String]) -> i32 {
    let gate = Gate::default();
    match args.first().map(String::as_str) {
        None | Some("list") => {
            let devices = gate.devices();
            if devices.is_empty() {
                println!("no trusted devices yet");
            }
            for (key, name, pinned, level) in devices {
                let note = if pinned {
                    ""
                } else {
                    "  (not QR-paired: no file access until it scans `wado qr`)"
                };
                println!("{level:<5} {name:<24} {}{note}", &key[..key.len().min(8)]);
            }
            0
        }
        Some("grant") if args.len() == 3 => match gate.grant(&args[1], &args[2]) {
            Ok(name) => {
                println!("{name}: files {}", args[2]);
                0
            }
            Err(e) => {
                eprintln!("{e}");
                1
            }
        },
        _ => {
            eprintln!("usage: wado files [list] | wado files grant <device> none|ro|rw");
            2
        }
    }
}
