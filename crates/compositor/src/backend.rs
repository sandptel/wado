//! This compositor as the platform's video backend (`wado_platform::video::VideoBackend`): wado's
//! own session, run on the thread that builds it, driven through [`CompositorHandles`].
//!
//! Nothing about the loop changes: [`Smithay::build`] is [`crate::build`], and its runner is the
//! loop `main` always ran, with the same post-dispatch flush. What the trait adds is that the
//! server asks a backend for its loop instead of naming calloop — so another backend (a mirror,
//! a macOS virtual display) can stand where this one does.

use calloop::EventLoop;
use tokio::sync::mpsc;
use wado_platform::Res;
use wado_platform::video::{Encoder, Model, Runner, Thread, VideoBackend};

use crate::{CompositorHandles, FrameMsg, Wado};

/// The headless Smithay compositor.
pub struct Smithay;

impl VideoBackend for Smithay {
    type Frame = FrameMsg;
    type Handles = CompositorHandles;

    fn model(&self) -> Model {
        Model::OwnSession
    }

    /// The loop owns the Wayland display and the GL context it made; it runs where it was built.
    /// `Main` because that is where the daemon has always run it.
    fn thread(&self) -> Thread {
        Thread::Main
    }

    /// x264 always; VA-API when an encoder opens and encodes a frame (probed once, cached).
    fn encoders(&self) -> Vec<Encoder> {
        let mut found = Vec::new();
        if crate::encode::ffmpeg::probe::vaapi_available() {
            found.push(Encoder {
                name: "vaapi",
                hardware: true,
            });
        }
        found.push(Encoder {
            name: "x264",
            hardware: false,
        });
        found
    }

    fn build(
        self: Box<Self>,
        frames: mpsc::Sender<FrameMsg>,
    ) -> Res<(Box<dyn Runner>, CompositorHandles)> {
        let (event_loop, state, handles) = crate::build(frames).map_err(|e| e.to_string())?;
        Ok((Box::new(Loop { event_loop, state }), handles))
    }
}

/// The calloop loop and its state.
struct Loop {
    event_loop: EventLoop<'static, Wado>,
    state: Wado,
}

impl Runner for Loop {
    fn run(self: Box<Self>) -> Res<()> {
        let Loop {
            mut event_loop,
            mut state,
        } = *self;
        // Post-dispatch flush: calloop runs this after EVERY dispatch, so remote input
        // synthesized on the input channel reaches the app immediately instead of waiting
        // for the next render tick to flush it (which quantised input to the frame period).
        // One flush per loop iteration, however many sources fired — cheaper than flushing
        // per event and it covers commands and Wayland traffic too.
        event_loop
            .run(None, &mut state, |state| state.after_dispatch())
            .map_err(|e| format!("compositor loop: {e}"))
    }
}
