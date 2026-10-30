//! `wado <command>` — everything the binary does besides being the daemon.
//!
//! - [`landing`] — bare `wado`: version, what is running, the commands.
//! - [`status`] — what is running, read from outside the daemons.
//! - [`style`] — terminal colour.

pub mod claude;
pub mod countdown;
pub mod devices;
pub mod files;
pub mod landing;
pub mod msg;
pub mod passkey;
pub mod passkey_approve;
pub mod qr;
pub mod relay;
pub mod status;
pub mod style;
pub mod validate;

/// What `main` should do with the command line.
pub enum Run {
    /// A command ran; exit with this code.
    Exit(i32),
    /// `wado daemon [addr]`: be the daemon, with these arguments.
    Daemon(Vec<String>),
}

pub fn dispatch(args: &[String]) -> Run {
    let cmd = args.first().map(String::as_str);
    if cmd == Some("daemon") {
        return Run::Daemon(args[1..].to_vec());
    }
    // Commands read the config too — `pair` needs `public-relay`. A broken file is the
    // daemon's to report; here it falls back to the defaults quietly.
    if let Ok(l) = wado_config::load_or_init() {
        wado_config::live::install(l.config);
    }
    let rest = args.get(1..).unwrap_or(&[]);
    Run::Exit(match cmd {
        None | Some("help" | "-h" | "--help") => {
            landing::print();
            0
        }
        Some("version" | "-V" | "--version") => {
            println!("wado {}", landing::VERSION);
            0
        }
        Some("tui") => crate::tui::run(rest),
        // Gone with remote approval (`2026-10-27`); said plainly for the hands that remember it.
        Some("approve") => {
            eprintln!(
                "  devices are no longer approved — pair one with {}",
                style::accent("wado pair")
            );
            2
        }
        Some("devices") => devices::list(),
        Some("unpair") => devices::unpair(rest),
        Some("allow") => devices::allow(rest, true),
        Some("deny") => devices::allow(rest, false),
        Some("files") => files::run(rest),
        Some("pair" | "qr") => qr::run(rest),
        Some("msg") => msg::run(rest),
        Some("relay") => relay::run(rest),
        Some("passkey") => passkey::run(rest),
        Some("claude") => claude::run(rest),
        Some("claude-event") => crate::claude::hook_cli::run(),
        Some("validate") => validate::run(rest.first().map(String::as_str)),
        Some(other) => {
            eprintln!("  {} is not a wado command", style::bad(other));
            match landing::suggest(other) {
                Some(s) => eprintln!("  did you mean {}?", style::accent(&format!("wado {s}"))),
                None => eprintln!("  run {} to see them all", style::accent("wado")),
            }
            2
        }
    })
}
