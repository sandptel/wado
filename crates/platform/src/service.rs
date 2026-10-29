//! Isolation where it pays: a backend run on its own thread behind a [`Port`] — commands in,
//! state out — and started again if it panics.
//!
//! Most capabilities are plain trait objects. This is for the few whose calls can hang or crash
//! (a stuck system bus, a native video stack): on a thread of their own, a hung call stalls only
//! that service, and a panic costs the command in flight, not the daemon. The command lane
//! outlives restarts, so callers never notice beyond that one command.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use tokio::sync::{mpsc, watch};

/// A backend that runs on its own thread.
pub trait Service: Send + 'static {
    type Cmd: Send + 'static;
    type State: Send + Sync + 'static;

    /// Serve commands until the lane closes, publishing state as it changes. A panic restarts
    /// the service with a fresh backend and the same lanes.
    fn run(
        &mut self,
        cmds: &mut mpsc::UnboundedReceiver<Self::Cmd>,
        state: &watch::Sender<Self::State>,
    );
}

/// The caller's side of a service: send commands, read state.
pub struct Port<C, S> {
    cmds: mpsc::UnboundedSender<C>,
    state: watch::Receiver<S>,
    restarts: Arc<AtomicU32>,
}

impl<C, S> Clone for Port<C, S> {
    fn clone(&self) -> Self {
        Port {
            cmds: self.cmds.clone(),
            state: self.state.clone(),
            restarts: Arc::clone(&self.restarts),
        }
    }
}

impl<C, S> Port<C, S> {
    /// Queue a command. False once the service has stopped for good.
    pub fn send(&self, cmd: C) -> bool {
        self.cmds.send(cmd).is_ok()
    }

    /// The service's latest state.
    pub fn state(&self) -> watch::Receiver<S> {
        self.state.clone()
    }

    /// How many times the service has been restarted after a panic.
    pub fn restarts(&self) -> u32 {
        self.restarts.load(Ordering::Relaxed)
    }
}

/// Wait between a panic and the restart: long enough that a backend failing on every start
/// does not spin a core, short enough not to be noticed.
const BACKOFF: Duration = Duration::from_millis(200);

/// Start a service on a thread named `name`: `make` builds the backend, again after each panic,
/// and `on_panic` hears the panic's message (to log it). Stops when every [`Port`] is dropped.
pub fn spawn<B: Service>(
    name: &str,
    init: B::State,
    make: impl Fn() -> B + Send + 'static,
    on_panic: impl Fn(&str) + Send + 'static,
) -> std::io::Result<Port<B::Cmd, B::State>> {
    let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel();
    let (state_tx, state_rx) = watch::channel(init);
    let restarts = Arc::new(AtomicU32::new(0));
    let counted = Arc::clone(&restarts);
    std::thread::Builder::new()
        .name(name.to_string())
        .spawn(move || {
            loop {
                let mut backend = make();
                match catch_unwind(AssertUnwindSafe(|| backend.run(&mut cmd_rx, &state_tx))) {
                    Ok(()) => return,
                    Err(panic) => {
                        let msg = panic
                            .downcast_ref::<&str>()
                            .map(|s| s.to_string())
                            .or_else(|| panic.downcast_ref::<String>().cloned())
                            .unwrap_or_else(|| "unknown panic".into());
                        on_panic(&msg);
                        counted.fetch_add(1, Ordering::Relaxed);
                        if cmd_rx.is_closed() {
                            return;
                        }
                        std::thread::sleep(BACKOFF);
                    }
                }
            }
        })?;
    Ok(Port {
        cmds: cmd_tx,
        state: state_rx,
        restarts,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Adds what it is sent; panics on 0.
    struct Adder(u32);
    impl Service for Adder {
        type Cmd = u32;
        type State = u32;
        fn run(&mut self, cmds: &mut mpsc::UnboundedReceiver<u32>, state: &watch::Sender<u32>) {
            while let Some(n) = cmds.blocking_recv() {
                assert!(n != 0, "zero");
                self.0 += n;
                state.send_replace(self.0);
            }
        }
    }

    #[tokio::test]
    async fn a_panic_restarts_the_backend_and_keeps_the_lanes() {
        let heard = Arc::new(std::sync::Mutex::new(Vec::new()));
        let h = Arc::clone(&heard);
        let port = spawn(
            "adder",
            0,
            || Adder(100),
            move |m| h.lock().unwrap().push(m.to_string()),
        )
        .unwrap();
        let mut state = port.state();
        port.send(1);
        state.changed().await.unwrap();
        assert_eq!(*state.borrow_and_update(), 101);
        port.send(0); // panics: a fresh Adder(100) takes over
        port.send(5);
        state.changed().await.unwrap();
        assert_eq!(
            *state.borrow_and_update(),
            105,
            "the next command reached the new backend"
        );
        assert_eq!(port.restarts(), 1);
        assert_eq!(heard.lock().unwrap().as_slice(), ["zero"]);
    }
}
