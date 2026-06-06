use std::{ffi::OsString, sync::Arc};

use smithay::{
    backend::renderer::{
        damage::OutputDamageTracker,
        gles::{GlesRenderbuffer, GlesRenderer},
    },
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

use crate::{encode::x264enc::X264Encoder, sink::FrameSink};

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
    pub renderbuffer: Option<GlesRenderbuffer>,
    pub damage_tracker: Option<OutputDamageTracker>,
    pub encoder: Option<X264Encoder>,
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
        seat.add_pointer();

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
            renderbuffer: None,
            damage_tracker: None,
            encoder: None,
            frame_sink: None,
            output: None,
            output_global: None,
            render_timer_token: None,
            app_processes: Vec::new(),
            session_active: false,
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
