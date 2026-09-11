//! Route panics through `tracing` so they land in the log like everything else.
//!
//! The default hook writes to stderr with no timestamp, no target and no level. In a daemon
//! whose stderr is appended to a log full of structured lines, that makes a panic **less**
//! visible than an ordinary INFO — which is how a `&text[..120]` unwind that wedged every
//! connection in a room sat unnoticed while three other causes were investigated.
//!
//! A panicking task is not a crash here: tokio catches it, the task dies, and the process
//! carries on serving everyone else. That is exactly why it has to be logged loudly — nothing
//! else about the process will look wrong afterwards.

/// Install the hook. Call once, first thing in `main`, after logging is initialised.
pub fn install() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let location = info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
            .unwrap_or_else(|| "unknown location".into());
        let msg = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| (*s).to_owned())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "<non-string panic payload>".into());
        tracing::error!(
            thread = std::thread::current().name().unwrap_or("unnamed"),
            %location,
            "PANIC: {msg} — this task is dead; anything it owned is no longer being serviced"
        );
        // Still run the default hook: it prints the backtrace when RUST_BACKTRACE is set,
        // which is the half worth keeping.
        default(info);
    }));
}
