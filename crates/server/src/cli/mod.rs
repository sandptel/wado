//! `wado <subcommand>` — everything the binary does besides being the daemon.

pub mod msg;
pub mod validate;

/// Run a subcommand if `args` names one, returning its exit code. `None`: no subcommand, run
/// the daemon.
pub fn dispatch(args: &[String]) -> Option<i32> {
    match args.first().map(String::as_str) {
        Some("msg") => Some(msg::run(&args[1..])),
        Some("validate") => Some(validate::run(args.get(1).map(String::as_str))),
        _ => None,
    }
}
