//! `wado-relay` — WebSocket signaling broker.
//!
//! Run it on any VPS with a public IP. All wado-server instances register here;
//! all wado-client connections join here; the relay brokers SDP + ICE exchange
//! so neither party needs to be port-forwarded. Media never flows through the relay
//! — only signaling (and session-control) messages do.
//!
//! ## Routes
//! - `WS /register`           — server registration and message inbox.
//! - `WS /join/:server_id`    — client join + post-handshake message forwarding.
//! - `GET /health`            — JSON health check `{ servers, rooms }`.

use std::net::SocketAddr;

use axum::extract::State;
use axum::response::Json;
use axum::routing::get;
use axum::Router;
use clap::Parser;
use serde_json::json;
use tracing::info;
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

mod config;
mod error;
mod registry;
mod room;
mod signaling;

pub use config::RelayConfig;
pub use registry::ServerRegistry;
pub use room::RoomStore;

/// Shared state threaded through every axum handler via `State<AppState>`.
#[derive(Clone)]
pub struct AppState {
    pub registry: ServerRegistry,
    pub rooms: RoomStore,
    pub config: RelayConfig,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cfg = RelayConfig::parse();

    // RUST_LOG takes priority; --log-level is the fallback.
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(&cfg.log_level));
    tracing_subscriber::registry().with(filter).with(fmt::layer()).init();

    let state = AppState {
        registry: ServerRegistry::new(),
        rooms: RoomStore::new(),
        config: cfg.clone(),
    };

    let app = Router::new()
        .route("/register", get(signaling::handle_register))
        .route("/join/:remote_id", get(signaling::handle_join))
        .route("/health", get(health))
        .with_state(state);

    let addr: SocketAddr = cfg.bind.parse().expect("invalid bind address");
    info!(%addr, "wado-relay listening — register: WS /register — join: WS /join/:id");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>()).await?;

    Ok(())
}

async fn health(State(state): State<AppState>) -> Json<serde_json::Value> {
    Json(json!({
        "status": "ok",
        "servers": state.registry.count(),
        "rooms": state.rooms.count(),
    }))
}
