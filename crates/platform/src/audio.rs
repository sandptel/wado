//! The computer's sound: outputs and what plays to them, and the output that plays to the
//! viewer ("This phone").

use tokio::sync::watch;
use wado_protocol::host::HostAudio;

use crate::{Fut, Res};

/// Outputs, streams, volume. State is the wire's [`HostAudio`] as the provider sees it — with
/// every output listed; which ones a viewer is shown is the caller's policy.
pub trait AudioMixer: Send + Sync {
    /// The latest state, published.
    fn state(&self) -> watch::Receiver<HostAudio>;
    /// Make the published state no older than `within` (see [`crate::Published::fresh`]).
    fn fresh(&self, within: std::time::Duration) -> Fut<'_, ()>;
    /// An output's or a stream's volume, 0..1.5 (the slider; above 1 is boost).
    fn set_volume(&self, id: u32, volume: f32) -> Fut<'_, Res<()>>;
    fn set_mute(&self, id: u32, muted: bool) -> Fut<'_, Res<()>>;
    /// Make the output named `name` the default — where what starts next plays.
    fn set_default(&self, name: String) -> Fut<'_, Res<()>>;
    /// Move one stream to the output named `sink`.
    fn move_stream(&self, id: u32, sink: String) -> Fut<'_, Res<()>>;
}

/// The output that plays to the viewer. It exists only while a handle to it does.
pub trait ViewerSink: Send + Sync {
    /// The output's fixed name for this run — apps are pointed at it before it exists, and it
    /// keeps the name while it comes and goes.
    fn name(&self) -> String;
    /// Make the output, shown as `description`. Dropping the handle removes it — and a provider
    /// should make sure it goes even if the daemon dies.
    fn open(&self, description: String) -> Res<Box<dyn SinkHandle>>;
}

/// A viewer sink that exists. Dropping it removes it.
pub trait SinkHandle: Send {}
