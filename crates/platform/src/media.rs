//! Media players: what is playing, and play/pause, next, previous, seek, shuffle, loop.

use tokio::sync::watch;
use wado_protocol::host::{MediaOp, Player};

use crate::{Fut, Res};

/// Every media player, playing ones first. A player's `stream` (the audio stream it plays on) is
/// the provider's to fill, from the mixer's state as of this refresh.
pub trait MediaPlayers: Send + Sync {
    fn state(&self) -> watch::Receiver<Vec<Player>>;
    fn fresh(&self, within: std::time::Duration) -> Fut<'_, ()>;
    /// Do `op` to the player at `bus` (its [`Player::bus`]).
    fn act(&self, bus: String, op: MediaOp) -> Fut<'_, Res<()>>;
}
