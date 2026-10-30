//! Runs one [`Link`] on the tokio runtime: the only place a link does I/O. It dials through a
//! [`Connector`], feeds frames, commands and timers to [`step`], and carries out what comes back.
//! What the link reports goes out as [`LinkOut`]; the core turns that into events.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc;
use tokio::time::Instant;
use wado_protocol::relay::RelayMsg;

use super::outbox::Class;
use super::state::{step, Action, Input, Link, LinkState, Timer};
use crate::relay::transport::{Connector, Transport};

/// How long a dial may take before it counts as unreachable.
const DIAL_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug)]
pub enum LinkCmd {
    Send {
        text: String,
        class: Class,
        resend: bool,
    },
    Stop,
}

#[derive(Debug, Clone)]
pub enum LinkOut {
    State(LinkState),
    Msg(RelayMsg),
    SavePin(String),
    Joined { caps: Vec<String>, instance: String },
}

/// Milliseconds on the link's clock (a monotonic one, from when the core started).
pub type Clock = Arc<dyn Fn() -> u64 + Send + Sync>;

async fn recv(t: &mut Option<Box<dyn Transport>>) -> Option<std::io::Result<String>> {
    match t {
        Some(t) => t.recv().await,
        None => std::future::pending().await,
    }
}

/// Run `link` until it is stopped. `out` closing (the core gone) stops it too.
pub async fn run(
    mut link: Link,
    connector: Arc<dyn Connector>,
    out: mpsc::UnboundedSender<LinkOut>,
    mut cmds: mpsc::UnboundedReceiver<LinkCmd>,
    clock: Clock,
) {
    let mut transport: Option<Box<dyn Transport>> = None;
    let mut timers: BTreeMap<Timer, Instant> = BTreeMap::new();
    let mut todo: Vec<Action> = step(&mut link, clock(), Input::Start);
    loop {
        // Carry out what the link asked for; each may feed it more input.
        let mut i = 0;
        while i < todo.len() {
            let a = todo[i].clone();
            i += 1;
            let more = match a {
                Action::Dial(url) => {
                    match tokio::time::timeout(DIAL_TIMEOUT, connector.connect(&url)).await {
                        Ok(Ok(t)) => {
                            transport = Some(t);
                            step(&mut link, clock(), Input::Opened)
                        }
                        _ => step(&mut link, clock(), Input::DialFailed),
                    }
                }
                Action::Send(text) => {
                    let ok = match transport.as_mut() {
                        Some(t) => t.send(text).await.is_ok(),
                        None => true,
                    };
                    if ok {
                        Vec::new()
                    } else {
                        transport = None;
                        step(&mut link, clock(), Input::Closed)
                    }
                }
                Action::Close(code) => {
                    if let Some(mut t) = transport.take() {
                        t.close(code).await;
                    }
                    Vec::new()
                }
                Action::Arm(t, ms) => {
                    timers.insert(t, Instant::now() + Duration::from_millis(ms));
                    Vec::new()
                }
                Action::Disarm(t) => {
                    timers.remove(&t);
                    Vec::new()
                }
                Action::State(s) => {
                    let _ = out.send(LinkOut::State(s));
                    Vec::new()
                }
                Action::Msg(m) => {
                    let _ = out.send(LinkOut::Msg(m));
                    Vec::new()
                }
                Action::SavePin(p) => {
                    let _ = out.send(LinkOut::SavePin(p));
                    Vec::new()
                }
                Action::Joined { caps, instance } => {
                    let _ = out.send(LinkOut::Joined { caps, instance });
                    Vec::new()
                }
                Action::Halt => return,
            };
            todo.extend(more);
        }
        todo.clear();
        if out.is_closed() {
            todo = step(&mut link, clock(), Input::Stop);
            continue;
        }
        let next = timers
            .iter()
            .min_by_key(|(_, at)| **at)
            .map(|(t, at)| (*t, *at));
        let sleep = async {
            match next {
                Some((_, at)) => tokio::time::sleep_until(at).await,
                None => std::future::pending().await,
            }
        };
        tokio::select! {
            r = recv(&mut transport) => {
                todo = match r {
                    Some(Ok(text)) => step(&mut link, clock(), Input::Frame(text)),
                    _ => {
                        transport = None;
                        step(&mut link, clock(), Input::Closed)
                    }
                };
            }
            c = cmds.recv() => {
                todo = match c {
                    Some(LinkCmd::Send { text, class, resend }) => {
                        step(&mut link, clock(), Input::Send { text, class, resend })
                    }
                    Some(LinkCmd::Stop) | None => step(&mut link, clock(), Input::Stop),
                };
            }
            _ = sleep => {
                if let Some((t, _)) = next {
                    timers.remove(&t);
                    todo = step(&mut link, clock(), Input::Timer(t));
                }
            }
        }
    }
}
