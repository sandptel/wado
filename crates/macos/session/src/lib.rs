//! The compositor boundary on macOS, with no session behind it yet.
//!
//! The server drives a session through `wado-compositor`'s names: [`CompositorCommand`], the
//! command and input senders, [`CompositorHandles`] and [`FrameMsg`]. On Linux that is the Smithay
//! compositor. On macOS this crate gives the same names, and its loop answers every session
//! command with [`COMING`] — so the rest of the daemon (files, clipboard, the pad, shells) runs
//! unchanged while the macOS session is designed. The server picks between the two in
//! `crates/server/src/compositor.rs`, and nowhere else.

use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

use tokio::sync::{mpsc, oneshot, watch};
use wado_platform::Res;
use wado_platform::video::{Encoder, Model, Runner, Thread, VideoBackend};
use wado_protocol::{SessionConfig, SessionInfo, WindowAction};

pub use wado_protocol::{InputEvent, StageTimings};

/// What every session request is told on a Mac, for now.
pub const COMING: &str =
    "A session on this Mac is coming to macOS — files, clipboard, the pad and shells work today.";

/// A request for the session. The same variants as the Linux compositor's, so the server's
/// code is one code; here only the ones with a reply do anything.
pub enum CompositorCommand {
    Start {
        config: SessionConfig,
        reply: oneshot::Sender<Result<SessionInfo, String>>,
    },
    Stop,
    Reconfigure {
        config: SessionConfig,
        reply: oneshot::Sender<Result<SessionInfo, String>>,
    },
    Status {
        reply: oneshot::Sender<Option<SessionInfo>>,
    },
    Launch {
        command: String,
    },
    RunningApps {
        reply: oneshot::Sender<Vec<String>>,
    },
    HitWindow {
        x: f64,
        y: f64,
        reply: oneshot::Sender<Option<hit::HitWindow>>,
    },
    AudioSink(Option<String>),
    SetClipboard(clipboard::Clip),
    ForceKeyframe,
    Window(WindowAction),
    ViewerStrain(bool),
    ViewerAttached(bool),
    ViewerVisible(bool),
}

/// The server names `control::CompositorCommand` in one place.
pub mod control {
    pub use super::CompositorCommand;
}

/// The command lane. A plain channel: there is no event loop to wake.
pub type CommandSender = std::sync::mpsc::Sender<CompositorCommand>;

/// Input for a session. Nothing reads it on a Mac yet; it is accepted and dropped, so a viewer
/// that sends picture input sees no error.
pub use input::{InputItem, InputSender, Origin, PRIMARY};

pub mod input {
    use std::sync::mpsc::Sender;

    use wado_protocol::InputEvent;

    pub type Origin = u16;
    pub const PRIMARY: Origin = 0;

    #[derive(Debug, Clone)]
    pub enum InputItem {
        Event(Origin, InputEvent),
        Gone(Origin),
        Lead(Origin),
    }

    #[derive(Debug)]
    pub struct Closed;

    #[derive(Clone)]
    pub struct InputSender(pub(crate) Sender<InputItem>);

    impl InputSender {
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

    pub mod gamepad {
        /// Virtual controllers open right now: never any on a Mac.
        pub fn live_count() -> usize {
            0
        }
    }
}

pub mod clipboard {
    pub use wado_platform::clip::Clip;
}

/// The window under a point, and the menu open on the focused window — session-only facts.
pub mod hit {
    #[derive(Debug, Clone, PartialEq)]
    pub struct HitWindow {
        pub a11y: String,
        pub pid: i32,
        pub title: String,
        pub local: (f64, f64),
        pub origin: (f64, f64),
        pub factor: f64,
        pub output: (f64, f64),
    }

    #[derive(Debug, Clone, PartialEq)]
    pub struct MenuSpot {
        pub window: HitWindow,
        pub rect: (f64, f64, f64, f64),
    }
}

/// An encoded access unit for the pump. Never produced on a Mac yet.
#[derive(Debug)]
pub struct FrameMsg {
    pub data: Vec<u8>,
    pub duration: Duration,
    pub queued_at: Instant,
}

#[derive(Debug, thiserror::Error)]
pub enum CompositorError {
    #[error("{0}")]
    Unavailable(String),
}

/// What the server holds of the session. The same fields as on Linux.
pub struct CompositorHandles {
    pub commands: CommandSender,
    pub input: InputSender,
    pub timings: watch::Receiver<StageTimings>,
    pub text_input: watch::Receiver<bool>,
    pub shedding: watch::Receiver<u32>,
    pub windows: watch::Receiver<wado_protocol::WindowList>,
    pub menu: watch::Receiver<Option<hit::MenuSpot>>,
    pub clipboard: watch::Receiver<clipboard::Clip>,
    pub app_bus: watch::Receiver<Option<String>>,
}

/// The senders behind the handles' watches: kept by the loop so the receivers stay open.
struct Held {
    _timings: watch::Sender<StageTimings>,
    _text_input: watch::Sender<bool>,
    _shedding: watch::Sender<u32>,
    _windows: watch::Sender<wado_protocol::WindowList>,
    _menu: watch::Sender<Option<hit::MenuSpot>>,
    _clipboard: watch::Sender<clipboard::Clip>,
    _app_bus: watch::Sender<Option<String>>,
}

/// The handles, and the loop that answers them.
fn parts() -> (Loop, CompositorHandles) {
    let (commands, rx) = channel();
    let (input_tx, input_rx) = channel();
    let (timings, timings_rx) = watch::channel(StageTimings::default());
    let (text_input, text_input_rx) = watch::channel(false);
    let (shedding, shedding_rx) = watch::channel(1);
    let (windows, windows_rx) = watch::channel(wado_protocol::WindowList::default());
    let (menu, menu_rx) = watch::channel(None);
    let (clipboard, clipboard_rx) = watch::channel(clipboard::Clip::default());
    let (app_bus, app_bus_rx) = watch::channel(None);
    let held = Held {
        _timings: timings,
        _text_input: text_input,
        _shedding: shedding,
        _windows: windows,
        _menu: menu,
        _clipboard: clipboard,
        _app_bus: app_bus,
    };
    // Input is accepted and dropped: a thread drains it so the channel never grows.
    std::thread::Builder::new()
        .name("session-input".into())
        .spawn(move || for _ in input_rx.iter() {})
        .expect("spawn the input drain");
    let handles = CompositorHandles {
        commands,
        input: InputSender(input_tx),
        timings: timings_rx,
        text_input: text_input_rx,
        shedding: shedding_rx,
        windows: windows_rx,
        menu: menu_rx,
        clipboard: clipboard_rx,
        app_bus: app_bus_rx,
    };
    (Loop { rx, _held: held }, handles)
}

struct Loop {
    rx: Receiver<CompositorCommand>,
    _held: Held,
}

impl Runner for Loop {
    fn run(self: Box<Self>) -> Res<()> {
        for cmd in self.rx.iter() {
            answer(cmd);
        }
        Ok(())
    }
}

/// One command's answer. Every reply-carrying request is answered, so no caller waits forever.
fn answer(cmd: CompositorCommand) {
    match cmd {
        CompositorCommand::Start { reply, .. } | CompositorCommand::Reconfigure { reply, .. } => {
            let _ = reply.send(Err(COMING.to_string()));
        }
        CompositorCommand::Status { reply } => {
            let _ = reply.send(None);
        }
        CompositorCommand::RunningApps { reply } => {
            let _ = reply.send(Vec::new());
        }
        CompositorCommand::HitWindow { reply, .. } => {
            let _ = reply.send(None);
        }
        _ => {}
    }
}

/// The session backend on macOS: none yet.
pub mod backend {
    use super::*;

    pub struct Session;

    impl VideoBackend for Session {
        type Frame = FrameMsg;
        type Handles = CompositorHandles;

        fn model(&self) -> Model {
            Model::Mirror
        }

        fn thread(&self) -> Thread {
            Thread::Any
        }

        fn encoders(&self) -> Vec<Encoder> {
            Vec::new()
        }

        fn build(
            self: Box<Self>,
            _frames: mpsc::Sender<FrameMsg>,
        ) -> Res<(Box<dyn Runner>, CompositorHandles)> {
            let (lp, handles) = parts();
            Ok((Box::new(lp), handles))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn running() -> CompositorHandles {
        let (lp, handles) = parts();
        std::thread::spawn(move || Box::new(lp).run());
        handles
    }

    #[test]
    fn start_is_refused_as_coming() {
        let h = running();
        let (reply, rx) = oneshot::channel();
        h.commands
            .send(CompositorCommand::Start {
                config: serde_json::from_str(
                    r#"{"width":390,"height":844,"fps":60,"quality":"balanced"}"#,
                )
                .unwrap(),
                reply,
            })
            .unwrap();
        let err = rx.blocking_recv().unwrap().unwrap_err();
        assert!(err.contains("coming to macOS"), "{err}");
    }

    #[test]
    fn status_is_none() {
        let h = running();
        let (reply, rx) = oneshot::channel();
        h.commands
            .send(CompositorCommand::Status { reply })
            .unwrap();
        assert!(rx.blocking_recv().unwrap().is_none());
    }

    #[test]
    fn running_apps_is_empty() {
        let h = running();
        let (reply, rx) = oneshot::channel();
        h.commands
            .send(CompositorCommand::RunningApps { reply })
            .unwrap();
        assert!(rx.blocking_recv().unwrap().is_empty());
    }

    #[test]
    fn hit_window_is_none() {
        let h = running();
        let (reply, rx) = oneshot::channel();
        h.commands
            .send(CompositorCommand::HitWindow {
                x: 0.5,
                y: 0.5,
                reply,
            })
            .unwrap();
        assert!(rx.blocking_recv().unwrap().is_none());
    }

    #[test]
    fn input_is_accepted() {
        let h = running();
        assert!(h.input.gone(PRIMARY).is_ok());
    }
}
