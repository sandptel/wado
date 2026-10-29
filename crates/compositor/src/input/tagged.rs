//! The input crossing: every event tagged with the device it came from, so the
//! [`arbiter`](super::arbiter) can let several devices drive one seat. Still its own channel,
//! never the command channel (invariant 1).
//!
//! A device leaving and the session's lead are on this channel too, not on the command one: a
//! release must land *after* that device's last event, which only one ordered channel promises.

use smithay::reexports::calloop::channel::Sender;
use wado_protocol::InputEvent;

pub use super::arbiter::{Origin, PRIMARY};

/// One item on the input channel.
#[derive(Debug, Clone)]
pub enum InputItem {
    /// An event from a device.
    Event(Origin, InputEvent),
    /// The device is gone (left, displaced, its peer connection died): release what it held.
    Gone(Origin),
    /// The session's lead is now this device: only it may move a locked pointer.
    Lead(Origin),
}

/// The `Send`able sender for remote input into the calloop loop.
#[derive(Clone)]
pub struct InputSender(pub(crate) Sender<InputItem>);

/// The loop is gone (the compositor stopped).
#[derive(Debug)]
pub struct Closed;

impl InputSender {
    /// An event with no device worth telling apart — direct mode, a single viewer.
    pub fn send(&self, ev: InputEvent) -> Result<(), Closed> {
        self.send_from(PRIMARY, ev)
    }

    pub fn send_from(&self, origin: Origin, ev: InputEvent) -> Result<(), Closed> {
        self.0
            .send(InputItem::Event(origin, ev))
            .map_err(|_| Closed)
    }

    pub fn gone(&self, origin: Origin) -> Result<(), Closed> {
        self.0.send(InputItem::Gone(origin)).map_err(|_| Closed)
    }

    pub fn lead(&self, origin: Origin) -> Result<(), Closed> {
        self.0.send(InputItem::Lead(origin)).map_err(|_| Closed)
    }
}
