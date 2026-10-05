//! CLI configuration for `wado-relay`.

use clap::builder::styling::{AnsiColor, Effects, Styles};
use clap::Parser;

/// Bold cyan headings and commands, like `wado`'s own landing page.
const STYLES: Styles = Styles::styled()
    .header(AnsiColor::Cyan.on_default().effects(Effects::BOLD))
    .usage(AnsiColor::Cyan.on_default().effects(Effects::BOLD))
    .literal(AnsiColor::Cyan.on_default())
    .placeholder(AnsiColor::BrightBlack.on_default());

#[derive(Debug, Clone, Parser)]
#[command(
    name = "wado-relay",
    version = concat!(env!("CARGO_PKG_VERSION"), " · ", env!("WADO_GIT"), " · built ", env!("WADO_BUILT")),
    styles = STYLES,
    about = "▌wado-relay — where a phone and a wado computer find each other",
    long_about = "▌wado-relay — where a phone and a wado computer find each other\n\n\
                  wado daemons register under their Remote ID; devices join by that ID. The \
                  relay brokers the WebRTC handshake and carries control messages it cannot \
                  read (they are end-to-end sealed). Video never passes through it.",
    after_help = "\x1b[1;36mExamples\x1b[0m\n  \
                  wado-relay                                  listen on 0.0.0.0:4000\n  \
                  wado-relay --bind 127.0.0.1:4000 --trust-proxy   behind cloudflared or Caddy\n  \
                  curl localhost:4000/health                  {servers, rooms}\n\n\
                  For local testing, scripts/rig.sh starts it with a tunnel and a daemon pool."
)]
pub struct RelayConfig {
    /// Address + port to bind the relay on.
    #[arg(long, default_value = "0.0.0.0:4000", help_heading = "Network")]
    pub bind: String,

    /// Log level filter (RUST_LOG syntax: `info`, `wado_relay=debug`, …).
    /// Overridden by the RUST_LOG environment variable if set.
    #[arg(long, default_value = "info", help_heading = "Logging")]
    pub log_level: String,

    /// Maximum number of active rooms (one per server_id) before new joins are
    /// rejected. 0 = unlimited.
    #[arg(long, default_value_t = 0, help_heading = "Limits")]
    pub max_rooms: usize,

    /// Believe `CF-Connecting-IP` / `X-Forwarded-For` for the client's address. Set it when
    /// every connection arrives through a proxy that overwrites those headers (Caddy, a
    /// cloudflared tunnel) — the join rate limit is per address, and without the flag every
    /// tunnelled client shares 127.0.0.1's bucket. Never set it on a relay exposed directly:
    /// the headers are client-settable, and believing them lets one machine look like many.
    #[arg(long, default_value_t = false, help_heading = "Network")]
    pub trust_proxy: bool,

    /// Joins an address may make in a burst before it is rate limited.
    #[arg(long, default_value_t = 20, help_heading = "Limits")]
    pub join_burst: u32,

    /// Seconds for an address to earn back one join after its burst is spent.
    #[arg(long, default_value_t = 3, help_heading = "Limits")]
    pub join_refill_secs: u64,
}
