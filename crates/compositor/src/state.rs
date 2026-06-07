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
    utils::{Logical, Point},
    wayland::{
        compositor::{CompositorClientState, CompositorState},
        output::OutputManagerState,
        selection::data_device::DataDeviceState,
        shell::xdg::XdgShellState,
        shm::ShmState,
        socket::ListeningSocketSource,
    },
};

use wado_protocol::Placement;

use crate::{capture::CaptureTarget, encode::encoder::VideoEncoder, sink::FrameSink};

pub struct Wado {
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
    pub shm_state: ShmState,
    pub output_manager_state: OutputManagerState,
    pub seat_state: SeatState<Wado>,
    pub data_device_state: DataDeviceState,
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
    pub frame_sink: Option<Box<dyn FrameSink>>,
    pub output: Option<Output>,
    /// The output's wl_output global, removed on session stop so a fresh session
    /// doesn't leave stale outputs advertised.
    pub output_global: Option<GlobalId>,
    /// Token for the render timer source, so it can be removed on session stop.
    pub render_timer_token: Option<RegistrationToken>,
    /// Applications launched inside the active session (the optional initial command
    /// plus any spawned at runtime). All are killed on session stop.
    pub app_processes: Vec<std::process::Child>,
    /// True between start_session and stop_session.
    pub session_active: bool,
    /// An in-progress compositor-managed window move (long-press-drag or "move mode"),
    /// driven by [`wado_protocol::InputEvent::WindowDrag`]. `None` when not moving.
    pub window_move: Option<WindowMove>,
    /// New-window placement policy (from `SessionConfig.window.placement`, applied at start).
    pub placement: Placement,
    /// When true, pointer hover also moves keyboard focus (`SessionConfig.input`).
    pub focus_follows_pointer: bool,
    /// Toplevels mapped but awaiting placement (Center/Cascade need the post-commit size).
    /// Drained by `Wado::apply_pending_placement`. See `placement.rs`.
    pub pending_placement: Vec<Window>,
    /// Running counter for `Placement::Cascade` step offsets.
    pub cascade_count: u32,
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
    pub fn new(event_loop: &mut EventLoop<'static, Self>, display: Display<Self>) -> Self {
        let start_time = std::time::Instant::now();

        let dh = display.handle();

        let compositor_state = CompositorState::new::<Self>(&dh);
        let xdg_shell_state = XdgShellState::new::<Self>(&dh);
        let shm_state = ShmState::new::<Self>(&dh, vec![]);
        let popups = PopupManager::default();
        let output_manager_state = OutputManagerState::new_with_xdg_output::<Self>(&dh);
        let data_device_state = DataDeviceState::new::<Self>(&dh);

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
            start_time,
            display_handle: dh,
            space,
            loop_signal,
            loop_handle,
            socket_name,
            compositor_state,
            xdg_shell_state,
            shm_state,
            output_manager_state,
            seat_state,
            data_device_state,
            popups,
            seat,
            renderer: None,
            gbm: None,
            capture: None,
            damage_tracker: None,
            encoder: None,
            frame_sink: None,
            output: None,
            output_global: None,
            render_timer_token: None,
            app_processes: Vec::new(),
            session_active: false,
            window_move: None,
            placement: Placement::default(),
            focus_follows_pointer: false,
            pending_placement: Vec::new(),
            cascade_count: 0,
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
                    unsafe {
                        display.get_mut().dispatch_clients(state).unwrap();
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
    fn initialized(&self, _client_id: ClientId) {}
    fn disconnected(&self, _client_id: ClientId, _reason: DisconnectReason) {}
}
