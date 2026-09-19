use std::panic::AssertUnwindSafe;
use std::{ffi::OsString, sync::Arc};

use smithay::{
    backend::renderer::{damage::OutputDamageTracker, gles::GlesRenderer},
    desktop::{PopupManager, Space, Window, WindowSurfaceType},
    input::{Seat, SeatState},
    output::Output,
    reexports::{
        calloop::{
            EventLoop, Interest, LoopHandle, LoopSignal, Mode, PostAction, RegistrationToken,
            generic::Generic,
        },
        wayland_server::{
            Display, DisplayHandle,
            backend::{ClientData, ClientId, DisconnectReason, GlobalId},
            protocol::wl_surface::WlSurface,
        },
    },
    utils::{Clock, Logical, Monotonic, Point},
    wayland::{
        compositor::{CompositorClientState, CompositorState},
        content_type::ContentTypeState,
        dmabuf::{DmabufGlobal, DmabufState},
        fractional_scale::FractionalScaleManagerState,
        output::OutputManagerState,
        presentation::PresentationState,
        pointer_constraints::PointerConstraintsState,
        pointer_gestures::PointerGesturesState,
        relative_pointer::RelativePointerManagerState,
        selection::data_device::DataDeviceState,
        shell::xdg::{XdgShellState, decoration::XdgDecorationState},
        shm::ShmState,
        single_pixel_buffer::SinglePixelBufferState,
        socket::ListeningSocketSource,
        viewporter::ViewporterState,
        xdg_activation::XdgActivationState,
    },
};

use wado_protocol::Placement;

use crate::{
    capture::CaptureTarget, conf::EncoderConfig, encode::encoder::VideoEncoder,
    encode::select::Tier, sink::FrameSink,
};

pub struct Wado {
    /// Per-stage render timing publisher. `None` only before [`crate::build`] installs it.
    pub timing: Option<crate::timing::StageTimer>,
    pub start_time: std::time::Instant,
    pub socket_name: OsString,
    pub display_handle: DisplayHandle,

    pub space: Space<Window>,
    pub loop_signal: LoopSignal,
    /// Handle for inserting/removing event sources at runtime (e.g. the per-session
    /// render timer). Captured from the event loop in [`Wado::new`].
    pub loop_handle: LoopHandle<'static, Wado>,

    // Smithay protocol state
    pub compositor_state: CompositorState,
    pub xdg_shell_state: XdgShellState,
    /// zxdg-decoration-v1. Every window is told the compositor decorates, and wado draws
    /// nothing — so windows are borderless. See `handlers/decoration.rs` for why.
    pub xdg_decoration_state: XdgDecorationState,
    pub shm_state: ShmState,
    pub output_manager_state: OutputManagerState,
    pub seat_state: SeatState<Wado>,
    pub data_device_state: DataDeviceState,
    /// wp-fractional-scale-v1. Without it `wl_output.scale` is the only channel and it is an
    /// integer, so a client asked for 1.5 is told 2, draws at 2x, and is composited as 1.5x —
    /// its buffer overhangs its own area and elements are visibly clipped.
    pub fractional_scale_state: FractionalScaleManagerState,
    pub pointer_gestures_state: PointerGesturesState,
    /// zwp-linux-dmabuf-v1. Unlike every other global here it is created **per session**,
    /// not at startup: its format list comes from the renderer, and there is no renderer
    /// until a session starts. That is safe only because every Wayland client here is
    /// spawned by the compositor *into* a running session, so none of them can bind before
    /// the global exists. See `headless::start_session`.
    pub dmabuf_state: DmabufState,
    pub dmabuf_global: Option<DmabufGlobal>,
    /// Whether this session has logged its first dmabuf import outcome yet.
    ///
    /// One line per session, not per buffer: the question "did any client take the GPU buffer
    /// path, or is everything still going through `wl_shm`?" is a yes/no, and answering it per
    /// buffer would bury it at 90 frames a second. Without this the only way to find out was
    /// to catch a live session and count dmabuf fds in `/proc`, which is archaeology against a
    /// window that closes.
    pub dmabuf_logged: bool,
    /// wp-viewport. Its companion: a client drawing for a fractional scale needs to declare
    /// the destination size its buffer maps onto, or the rounding it just avoided reappears
    /// at composite time.
    pub viewporter_state: ViewporterState,
    /// xdg-activation-v1. The launcher's half of "raise the window I just started": a toolkit
    /// passes the token through `exec`, and without the global it has no way to ask at all.
    pub xdg_activation_state: XdgActivationState,
    /// wp-presentation. Answers "when was the frame I drew actually shown?" — the timestamp
    /// GTK and Chrome pace their animations against. wado has no scanout, so the honest answer
    /// is the instant the render tick finished compositing, with none of the
    /// `HwClock`/`HwCompletion`/`Vsync` flags set: those claim a display pipeline that does not
    /// exist here. See `headless::render_tick` for why a wrong answer is worse than none.
    pub presentation_state: PresentationState,
    /// wp-single-pixel-buffer-v1. A one-pixel solid-colour `wl_buffer` with no shm pool and no
    /// upload behind it. Toolkits use it for the thing that is most expensive to do the long
    /// way here: a full-screen opaque backdrop behind a dialog, which as a real buffer is a
    /// width × height allocation plus a texture upload every time it is damaged. The renderer
    /// already handles `BufferType::SinglePixel`, so advertising it is the whole change.
    pub single_pixel_buffer_state: SinglePixelBufferState,
    /// wp-content-type-v1. Advertised so apps can declare `Video`/`Game`/`Photo`; nothing reads
    /// the hint yet — see `handlers/content_type.rs` for why logging it is the whole point.
    pub content_type_state: ContentTypeState,
    /// zwp-text-input-v3, hand-rolled as an observer — see `handlers/text_input.rs` for why it
    /// is not smithay's. This is what turns "an app focused a text field" into a soft keyboard
    /// on the phone, instead of a tap on ⌨ every time.
    pub text_inputs: crate::handlers::text_input::TextInputs,
    /// Publishes [`Self::text_inputs`]'s one bit to the server. Latest-value-wins, the same
    /// shape as the render timings — a viewer that missed an intermediate state does not care
    /// what it was, only what is true now.
    pub text_input_tx: tokio::sync::watch::Sender<bool>,
    /// How many render ticks out of every N are actually being rendered — `crate::congestion`'s
    /// divisor, 1 when nothing is being shed.
    ///
    /// Published for the *viewer*, which otherwise measures the effect of a mitigation it does
    /// not know about and concludes the server stopped producing. Measured 2026-09-12 16:17:33:
    /// the strip read `bad the server — only 816 kbps arriving of 5.7 Mbps` about a frame rate
    /// the phone itself had asked for three seconds earlier.
    pub shedding_tx: tokio::sync::watch::Sender<u32>,
    /// Per-surface content-type change log. Cleared on session stop.
    pub content_type_log: crate::handlers::content_type::ContentTypeLog,
    /// `CLOCK_MONOTONIC`, read for presentation timestamps. `start_time.elapsed()` is *not*
    /// interchangeable with this: frame callbacks take an arbitrary millisecond counter, but a
    /// presentation timestamp is compared by the client against its own reading of the clock id
    /// the global advertises — so an uptime-relative value would put every frame hours in the
    /// past.
    pub clock: Clock<Monotonic>,
    /// Per-output presentation sequence number: frames composited since the session started.
    pub frame_seq: u64,
    /// Whether this session has logged that a client actually collected presentation feedback.
    ///
    /// Same job as [`Self::dmabuf_logged`], and for the same reason: smithay **silently
    /// discards** a feedback callback whose clock id disagrees with the one the global
    /// advertised, so "no client is pacing on us" and "every timestamp we sent was thrown away"
    /// look identical without a line that fires on the positive branch.
    pub presentation_logged: bool,
    pub popups: PopupManager,
    pub seat: Seat<Self>,

    // Headless rendering pipeline — present only while a session is active
    // (set by headless::start_session, cleared by headless::stop_session).
    pub renderer: Option<GlesRenderer>,
    /// The GBM device backing the EGL display + dmabuf allocator. `Some` only when a DRM
    /// render node opened (gates the zero-copy DMA-BUF capture tier).
    pub gbm: Option<crate::capture::gpu::Gbm>,
    pub capture: Option<Box<dyn CaptureTarget>>,
    pub damage_tracker: Option<OutputDamageTracker>,
    pub encoder: Option<Box<dyn VideoEncoder>>,
    /// The active pipeline tier (for runtime downgrade-once) and the config used to build it.
    pub current_tier: Option<Tier>,
    pub encoder_config: Option<EncoderConfig>,
    /// What the running session's encoder actually opened, kept so a *later* caller can be told
    /// about a session it did not start.
    ///
    /// `start` returns this once and the reply goes to whoever asked. A viewer that reconnects —
    /// or a second device — has no way back to that answer, and "a session is already active" is
    /// not enough to decide whether to join it. See `CompositorCommand::Status`.
    pub encoder_report: Option<wado_protocol::EncoderReport>,
    /// Render-tick shedding when the pump cannot take the frames we are making.
    /// See `crate::congestion` — this is a mitigation, not bandwidth estimation.
    pub congestion: crate::congestion::Congestion,
    /// The viewer's latest word on whether its own decoder is keeping up. Nothing in this
    /// process can observe that, so it is the one congestion input that has to be told to us.
    /// A latched level, updated about once a second; see `CompositorCommand::ViewerStrain`.
    pub viewer_strained: bool,
    /// Whether anyone is actually watching. **The session's lifetime is not its viewer's.**
    ///
    /// Rendering and encoding for nobody is the reason a session with no viewer was expensive
    /// enough to need reaping in the first place; pausing here is what lets the grace period be
    /// generous instead of a race the user keeps losing. Window state stays live either way —
    /// a paused tick still sends frame callbacks, so clients keep drawing and a reattach finds
    /// the desktop as it was, not frozen mid-frame.
    ///
    /// Starts `true` and is only ever lowered by an explicit `ViewerAttached(false)`. That is
    /// deliberate: the direct HTTP path never sends the command at all, so it keeps exactly its
    /// old behaviour rather than going black on a flag it does not know about.
    pub viewer_attached: bool,
    /// The scale the current `Output` was built with. Stored because `reconfigure_session` has to
    /// answer "did the output's shape actually change?", and the scale is the one input to that
    /// which `encoder_config` does not carry.
    /// Whether the viewer's page is on screen. `true` unless a viewer says otherwise, so the
    /// direct transport — which never sends it — behaves exactly as before.
    pub viewer_visible: bool,
    pub output_scale: f32,
    /// Whether the "pipeline is incomplete" warning has already been said. The render tick runs
    /// up to 240 times a second, so a warning that is not latched is a flood.
    pub pipeline_gap_logged: bool,
    pub frame_sink: Option<Box<dyn FrameSink>>,
    pub output: Option<Output>,
    /// The output's wl_output global, removed on session stop so a fresh session
    /// doesn't leave stale outputs advertised.
    pub output_global: Option<GlobalId>,
    /// Token for the render timer source, so it can be removed on session stop.
    pub render_timer_token: Option<RegistrationToken>,
    /// Applications launched inside the active session (the optional initial command
    /// plus any spawned at runtime). All are killed on session stop.
    pub app_processes: Vec<crate::proc::Launched>,
    /// The environment launched applications get, decided at session start from
    /// [`wado_protocol::SessionConfig::isolate_apps`] and from whether a private bus started.
    pub app_env: crate::session_env::AppEnv,
    /// The session's own D-Bus daemon, when it is isolated and one could be started. Killed
    /// with the session — see [`crate::session_env::bus`].
    pub app_bus: Option<crate::session_env::bus::SessionBus>,
    /// The session's own X server, when it was asked for and could be started. Killed with the
    /// session — see [`crate::session_env::xwayland`].
    pub app_x: Option<crate::session_env::xwayland::XServer>,
    /// True between start_session and stop_session.
    pub session_active: bool,
    /// An in-progress compositor-managed window move (long-press-drag or "move mode"),
    /// driven by [`wado_protocol::InputEvent::WindowDrag`]. `None` when not moving.
    pub window_move: Option<WindowMove>,
    /// Axis events in the current finger-scroll gesture, reset when it ends. Counted so one
    /// log line per gesture can report its length — a gesture that produced three events is a
    /// different fault from one that produced three hundred.
    pub scroll_events: u32,
    /// Whether a `zwp_pointer_gestures_v1` pinch is currently open.
    ///
    /// Tracked on this side because the client's end event is the half that can go missing:
    /// a disconnect, a `resetInput`, or a third contact all drop the gesture without one.
    /// An un-ended pinch is not the same harmless thing as an un-stopped scroll axis —
    /// the toolkit stays in zoom mode, and the next begin lands on an already-open gesture.
    /// Windows outlive sessions here, so an orphan would survive into the next one.
    pub pinch_open: bool,

    /// New-window placement policy (from `SessionConfig.window.placement`, applied at start).
    pub placement: Placement,
    /// When true, pointer hover also moves keyboard focus (`SessionConfig.input`).
    pub focus_follows_pointer: bool,
    /// Where each maximized window was before it was maximized, so restore has somewhere to
    /// go back to. Only maximized windows appear here; the entry is removed on restore, and
    /// a window maximized at map time by `Placement::Maximized` never has one.
    pub pre_maximize: std::collections::HashMap<Window, crate::window::PreMaximize>,
    /// Toplevels mapped but awaiting placement (Center/Cascade need the post-commit size).
    /// Drained by `Wado::apply_pending_placement`. See `placement.rs`.
    pub pending_placement: Vec<Window>,
    /// Running counter for `Placement::Cascade` step offsets.
    pub cascade_count: u32,
    /// The ring drawn around the focused window. Held here rather than rebuilt per frame so
    /// its buffers keep stable ids — see [`crate::glow`] for why that is a damage question and
    /// not a tidiness one.
    pub glow: crate::glow::Glow,
}

/// Tracks a compositor-driven interactive window move. Unlike the app-initiated CSD grabs
/// (see `grabs/touch_move_grab.rs`), this is a plain state machine fed by `WindowDrag`
/// events — it never involves a Smithay touch grab, so it can't fight touch routing.
pub struct WindowMove {
    /// The window being dragged.
    pub window: Window,
    /// Pointer location (logical) at the start of the move.
    pub start_ptr: Point<f64, Logical>,
    /// Window location (logical) at the start of the move.
    pub start_win: Point<i32, Logical>,
}

impl Wado {
    /// Write any pending Wayland protocol output to the clients' sockets.
    ///
    /// Must be called after EVERY event-loop dispatch, not just after rendering.
    /// Synthesizing remote input (see [`Wado::handle_remote_input`]) only queues events
    /// into each client's outgoing buffer; until this flush runs, the app has not
    /// actually been told anything. Flushing solely at the tail of the render tick
    /// quantises every remote input to the frame period with a variable phase — which is
    /// exactly what remote input "feeling jittery" is. The server wires this into the
    /// event loop's post-dispatch callback.
    ///
    /// Errors are intentionally swallowed: a dead or backed-up client socket is the
    /// Wayland layer's problem to reap, not a reason to disturb the render loop.
    /// Tell the server whether the focused app currently wants text input.
    ///
    /// Sent only on a change: `send_if_modified` is what keeps a per-focus-change call from
    /// waking the server on every window switch that changes nothing.
    pub fn publish_text_input(&mut self) {
        let active = self.text_inputs.active();
        self.text_input_tx.send_if_modified(|cur| {
            if *cur == active {
                false
            } else {
                *cur = active;
                tracing::debug!(active, "text input focus changed");
                true
            }
        });
    }

    pub fn flush_clients(&mut self) {
        let _ = self.display_handle.flush_clients();
    }

    pub fn new(event_loop: &mut EventLoop<'static, Self>, display: Display<Self>) -> Self {
        let start_time = std::time::Instant::now();

        let dh = display.handle();

        let compositor_state = CompositorState::new::<Self>(&dh);
        let xdg_shell_state = XdgShellState::new::<Self>(&dh);
        let xdg_decoration_state = XdgDecorationState::new::<Self>(&dh);
        let shm_state = ShmState::new::<Self>(&dh, vec![]);
        let popups = PopupManager::default();
        let output_manager_state = OutputManagerState::new_with_xdg_output::<Self>(&dh);
        let data_device_state = DataDeviceState::new::<Self>(&dh);
        // Advertised unconditionally: a client decides how to draw when it binds, long
        // before a session sets a scale, and a global that appears later is one most
        // toolkits will never look for again.
        let fractional_scale_state = FractionalScaleManagerState::new::<Self>(&dh);
        let viewporter_state = ViewporterState::new::<Self>(&dh);
        // Pinch and rotate for a two-finger touch gesture. Advertised unconditionally for
        // the same reason as the two above: a toolkit looks for its gesture global when it
        // binds the seat, and one that appears later is one it never asks for again.
        let pointer_gestures_state = PointerGesturesState::new::<Self>(&dh);
        // The two halves of pointer lock, and they are only useful together: engines look for
        // both before enabling relative mouse mode and fall back to warping if either is
        // missing. Unconditional for the same reason as the globals above — a toolkit asks once,
        // when it binds. Neither handle is kept: the returned value is only an id, and dropping
        // it does not retire the global.
        RelativePointerManagerState::new::<Self>(&dh);
        PointerConstraintsState::new::<Self>(&dh);
        let dmabuf_state = DmabufState::new();
        let xdg_activation_state = XdgActivationState::new::<Self>(&dh);
        let clock = Clock::<Monotonic>::new();
        // The clock id is part of the global: `OutputPresentationFeedback::presented` derives
        // the same id from the `Time<Monotonic>` it is handed and discards any callback whose
        // id disagrees, so these two must name the same clock.
        let presentation_state = PresentationState::new::<Self>(&dh, clock.id() as u32);
        let single_pixel_buffer_state = SinglePixelBufferState::new::<Self>(&dh);
        let content_type_state = ContentTypeState::new::<Self>(&dh);
        // Our own manager, not smithay's: see `handlers/text_input.rs`. Registering both would
        // advertise two globals of the same interface.
        dh.create_global::<Self, smithay::reexports::wayland_protocols::wp::text_input::zv3::server::zwp_text_input_manager_v3::ZwpTextInputManagerV3, _>(1, ());

        let mut seat_state = SeatState::new();
        let mut seat: Seat<Self> = seat_state.new_wl_seat(&dh, "headless");
        seat.add_keyboard(Default::default(), 200, 25).unwrap();
        // Touch is the primary remote input (see input/remote.rs). Pointer capability is
        // kept (harmless) but never driven — wado has no on-screen cursor.
        seat.add_pointer();
        seat.add_touch();

        let space = Space::default();
        let socket_name = Self::init_wayland_listener(display, event_loop);
        let loop_signal = event_loop.get_signal();
        let loop_handle = event_loop.handle();

        Self {
            timing: None,
            start_time,
            display_handle: dh,
            space,
            loop_signal,
            loop_handle,
            socket_name,
            compositor_state,
            xdg_shell_state,
            xdg_decoration_state,
            shm_state,
            output_manager_state,
            seat_state,
            data_device_state,
            fractional_scale_state,
            pointer_gestures_state,
            dmabuf_state,
            dmabuf_global: None,
            dmabuf_logged: false,
            viewporter_state,
            xdg_activation_state,
            presentation_state,
            single_pixel_buffer_state,
            content_type_state,
            content_type_log: Default::default(),
            text_inputs: Default::default(),
            text_input_tx: tokio::sync::watch::channel(false).0,
            shedding_tx: tokio::sync::watch::channel(1).0,
            clock,
            frame_seq: 0,
            presentation_logged: false,
            popups,
            seat,
            renderer: None,
            gbm: None,
            capture: None,
            damage_tracker: None,
            encoder: None,
            current_tier: None,
            encoder_config: None,
            encoder_report: None,
            congestion: Default::default(),
            viewer_strained: false,
            viewer_attached: true,
            viewer_visible: true,
            output_scale: 1.0,
            pipeline_gap_logged: false,
            frame_sink: None,
            output: None,
            output_global: None,
            render_timer_token: None,
            app_processes: Vec::new(),
            // No session, nothing launched. `start_session` replaces this before it launches
            // anything.
            app_env: crate::session_env::AppEnv::Host { x: None },
            app_bus: None,
            app_x: None,
            session_active: false,
            window_move: None,
            scroll_events: 0,
            pinch_open: false,
            placement: Placement::default(),
            focus_follows_pointer: false,
            pre_maximize: std::collections::HashMap::new(),
            pending_placement: Vec::new(),
            cascade_count: 0,
            glow: crate::glow::Glow::new(),
        }
    }

    fn init_wayland_listener(display: Display<Wado>, event_loop: &mut EventLoop<Self>) -> OsString {
        let listening_socket = ListeningSocketSource::new_auto().unwrap();
        let socket_name = listening_socket.socket_name().to_os_string();
        let loop_handle = event_loop.handle();

        loop_handle
            .insert_source(listening_socket, move |client_stream, _, state| {
                state
                    .display_handle
                    .insert_client(client_stream, Arc::new(ClientState::default()))
                    .unwrap();
            })
            .expect("Failed to init the wayland event source.");

        loop_handle
            .insert_source(
                Generic::new(display, Interest::READ, Mode::Level),
                |_, display, state| {
                    // The last event source without a panic guard, and the one fed by the
                    // least trustworthy input: every request from every Wayland client runs
                    // under this call. A panic here unwinds out of `event_loop.run` and kills
                    // the daemon — and because `main` unwinds rather than exiting, the session
                    // teardown that reaps launched applications never runs, so they survive the
                    // daemon that owned them. That is the exact leak `proc::terminate` and the
                    // SIGTERM handler exist to prevent, arriving by a path neither covers.
                    //
                    // The command and input sources are guarded the same way (see `lib.rs`).
                    // A misbehaving client is dropped; the compositor keeps running.
                    let caught = std::panic::catch_unwind(AssertUnwindSafe(|| unsafe {
                        display.get_mut().dispatch_clients(state)
                    }));
                    match caught {
                        Ok(Ok(_)) => {}
                        Ok(Err(e)) => tracing::warn!("wayland dispatch error: {e}"),
                        Err(_) => tracing::error!(
                            "compositor panicked dispatching a wayland client —                              the client's request was dropped, the session continues"
                        ),
                    }
                    Ok(PostAction::Continue)
                },
            )
            .unwrap();

        socket_name
    }

    pub fn surface_under(&self, pos: Point<f64, Logical>) -> Option<(WlSurface, Point<f64, Logical>)> {
        self.space.element_under(pos).and_then(|(window, location)| {
            window
                .surface_under(pos - location.to_f64(), WindowSurfaceType::ALL)
                .map(|(s, p)| (s, (p + location).to_f64()))
        })
    }
}

#[derive(Default)]
pub struct ClientState {
    pub compositor_state: CompositorClientState,
}

impl ClientData for ClientState {
    fn initialized(&self, client_id: ClientId) {
        tracing::debug!(?client_id, "wayland client connected");
    }

    /// **Say why a client went away.** This was an empty function, and the silence cost a whole
    /// debugging cycle on 2026-09-13: a session's application vanished within two seconds of a
    /// reconfigure, and nothing anywhere said whether it had exited on its own, been killed, or
    /// been disconnected by the compositor for a protocol error. Those are three different bugs
    /// with three different fixes and the log could not tell them apart.
    ///
    /// `ProtocolError` is the one that matters most: it means *we* killed it. libwayland
    /// disconnects a client that touches a dead object, and a client whose display dies exits —
    /// so a protocol error here shows up as an application mysteriously quitting, several layers
    /// away from whatever the compositor actually did wrong.
    fn disconnected(&self, client_id: ClientId, reason: DisconnectReason) {
        match reason {
            DisconnectReason::ProtocolError(err) => tracing::error!(
                ?client_id,
                interface = %err.object_interface,
                object = err.object_id,
                code = err.code,
                "disconnected a wayland client for a protocol error — this is the compositor's \
                 fault, not the application's: {}",
                err.message
            ),
            other => tracing::info!(?client_id, ?other, "wayland client disconnected"),
        }
    }
}
