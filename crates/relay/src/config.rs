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
}
