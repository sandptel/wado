//! The screen's backlight, as a level.

use tokio::sync::watch;

use crate::{Fut, Res};

/// The screen's backlight. The state is `None` when there is nothing to set (a desktop monitor).
pub trait Brightness: Send + Sync {
    /// 0..1, or `None`.
    fn state(&self) -> watch::Receiver<Option<f32>>;
    fn fresh(&self, within: std::time::Duration) -> Fut<'_, ()>;
    /// To `level`, 0..1. Never fully off — a black panel nobody can see to undo is not a level.
    fn set(&self, level: f32) -> Fut<'_, Res<()>>;
    /// One step brighter or dimmer, the way the laptop's own keys do it.
    fn step(&self, up: bool) -> Fut<'_, Res<()>>;
}
