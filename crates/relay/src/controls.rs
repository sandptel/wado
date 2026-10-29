//! Control-only links — a phone's widgets talking to a computer **without a seat**.
//!
//! A seat (see [`crate::room`]) pairs one device with one daemon's desktop, and a pool has only
//! as many as it has daemons. A widget wants none of that: just the computer's state and its
//! controls. So a control link is kept here instead, by room id, beside the seats and never
//! counted among them: it is not held when it drops, displaces nobody, cannot be taken over, and
//! nothing the daemon sends a seat's viewer reaches it. Its frames travel wrapped in `ctl` frames
//! so the daemon can tell its rooms apart on the one socket it has to the relay.

use std::sync::Arc;

use dashmap::DashMap;
use tokio::sync::{mpsc, oneshot};

struct Entry {
    instance_id: String,
    inbox_tx: mpsc::Sender<String>,
    /// Dropped to end the client's link (the daemon closed the room).
    _end: oneshot::Sender<()>,
}

#[derive(Default, Clone)]
pub struct ControlStore {
    inner: Arc<DashMap<String, Entry>>,
}

impl ControlStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// A control link on `instance_id`. The receiver resolves when the daemon ends it.
    pub fn open(
        &self,
        room_id: &str,
        instance_id: &str,
        inbox_tx: mpsc::Sender<String>,
    ) -> oneshot::Receiver<()> {
        let (end, ended) = oneshot::channel();
        self.inner.insert(
            room_id.to_string(),
            Entry {
                instance_id: instance_id.to_string(),
                inbox_tx,
                _end: end,
            },
        );
        ended
    }

    /// A frame from daemon `instance_id` for its control client in `room_id`. Only the daemon the
    /// link was opened to may write to it. False when there is no such link.
    pub async fn deliver(&self, instance_id: &str, room_id: &str, text: String) -> bool {
        let tx = match self.inner.get(room_id) {
            Some(e) if e.instance_id == instance_id => e.inbox_tx.clone(),
            _ => return false,
        };
        tx.send(text).await.is_ok()
    }

    /// The daemon ended the link: the client's socket is closed.
    pub fn end(&self, instance_id: &str, room_id: &str) {
        self.inner
            .remove_if(room_id, |_, e| e.instance_id == instance_id);
    }

    /// The client went: forget the link.
    pub fn remove(&self, room_id: &str) {
        self.inner.remove(room_id);
    }

    pub fn count(&self) -> usize {
        self.inner.len()
    }
}

#[cfg(test)]
mod tests {
    use super::ControlStore;
    use tokio::sync::mpsc;

    #[tokio::test]
    async fn only_the_owning_daemon_reaches_a_control_client() {
        let c = ControlStore::new();
        let (tx, mut rx) = mpsc::channel(4);
        let mut ended = c.open("r1", "id:1", tx);
        assert!(c.deliver("id:1", "r1", "hello".into()).await);
        assert_eq!(rx.recv().await.as_deref(), Some("hello"));
        // Another daemon of the pool cannot write into this link, or end it.
        assert!(!c.deliver("id:2", "r1", "x".into()).await);
        c.end("id:2", "r1");
        assert!(ended
            .try_recv()
            .is_err_and(|e| e == tokio::sync::oneshot::error::TryRecvError::Empty));
        assert!(!c.deliver("id:1", "nope", "x".into()).await);
        // Its own daemon ends it: the client's loop is told.
        c.end("id:1", "r1");
        assert!(ended.await.is_err(), "sender dropped = ended");
        assert_eq!(c.count(), 0);
    }
}
