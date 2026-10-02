//! CLI configuration for `wado-relay`.

use clap::Parser;

#[derive(Debug, Clone, Parser)]
#[command(
    name = "wado-relay",
    about = "wado relay — WebSocket signaling broker for WebRTC P2P connections.",
    long_about = "Runs as a public-facing broker on a VPS. \
                  wado-server instances register with a server_id + password. \
                  wado-client instances join a server_id; the relay brokers SDP/ICE \
                  exchange and forwards all session control messages."
)]
pub struct RelayConfig {
    /// Address + port to bind the relay on.
    #[arg(long, default_value = "0.0.0.0:4000")]
    pub bind: String,

    /// Log level filter (RUST_LOG syntax: `info`, `wado_relay=debug`, …).
    /// Overridden by the RUST_LOG environment variable if set.
    #[arg(long, default_value = "info")]
    pub log_level: String,

    /// Maximum number of active rooms (one per server_id) before new joins are
    /// rejected. 0 = unlimited.
    #[arg(long, default_value_t = 0)]
    pub max_rooms: usize,

    /// Believe `CF-Connecting-IP` / `X-Forwarded-For` for the client's address. Set it when
    /// every connection arrives through a proxy that overwrites those headers (Caddy, a
    /// cloudflared tunnel) — the join rate limit is per address, and without the flag every
    /// tunnelled client shares 127.0.0.1's bucket. Never set it on a relay exposed directly:
    /// the headers are client-settable, and believing them lets one machine look like many.
    #[arg(long, default_value_t = false)]
    pub trust_proxy: bool,

    /// Joins an address may make in a burst before it is rate limited.
    #[arg(long, default_value_t = 20)]
    pub join_burst: u32,

    /// Seconds for an address to earn back one join after its burst is spent.
    #[arg(long, default_value_t = 3)]
    pub join_refill_secs: u64,
}
