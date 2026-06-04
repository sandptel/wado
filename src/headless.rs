use std::time::Duration;

use smithay::{
    backend::{
        allocator::Fourcc,
        egl::{EGLContext, EGLDisplay, native::EGLSurfacelessDisplay},
        renderer::{
            Bind, Offscreen,
            damage::OutputDamageTracker,
            element::surface::WaylandSurfaceRenderElement,
            gles::{GlesRenderbuffer, GlesRenderer},
        },
    },
    output::{Mode, Output, PhysicalProperties, Subpixel},
    reexports::calloop::{EventLoop, timer::{TimeoutAction, Timer}},
    utils::{Buffer, Size, Transform},
};

use crate::{
    Wado,
    capture::mem::capture_frame,
    encode::x264enc::X264Encoder,
    sink::{FrameSink, udp::UdpSink},
};

/// Logical output resolution for M1. M5 will make this per-client.
const WIDTH: u32 = 1280;
const HEIGHT: u32 = 720;
const FPS: u32 = 60;

pub fn init_headless(
    event_loop: &mut EventLoop<Wado>,
    state: &mut Wado,
) -> Result<(), Box<dyn std::error::Error>> {
    // ── EGL / GLES renderer ───────────────────────────────────────────────────
    let egl_display = unsafe { EGLDisplay::new(EGLSurfacelessDisplay)? };
    let egl_context = EGLContext::new(&egl_display)?;
    let mut renderer = unsafe { GlesRenderer::new(egl_context)? };

    // ── Offscreen render target ───────────────────────────────────────────────
    let buf_size: Size<i32, Buffer> = (WIDTH as i32, HEIGHT as i32).into();
    let renderbuffer: GlesRenderbuffer = renderer.create_buffer(Fourcc::Abgr8888, buf_size)?;

    // ── Logical Output (no physical display) ─────────────────────────────────
    let mode = Mode {
        size: (WIDTH as i32, HEIGHT as i32).into(),
        refresh: (FPS * 1000) as i32,
    };
    let output = Output::new(
        "HEADLESS-1".to_string(),
        PhysicalProperties {
            size: (0, 0).into(),
            subpixel: Subpixel::Unknown,
            make: "wado".into(),
            model: "Headless".into(),
            serial_number: "0".into(),
        },
    );
    let _global = output.create_global::<Wado>(&state.display_handle);
    output.change_current_state(Some(mode), Some(Transform::Normal), None, Some((0, 0).into()));
    output.set_preferred(mode);
    state.space.map_output(&output, (0, 0));

    let damage_tracker = OutputDamageTracker::from_output(&output);

    // ── Encoder + Sink ────────────────────────────────────────────────────────
    let mut encoder = X264Encoder::new(WIDTH, HEIGHT, FPS)?;
    let headers = encoder.headers();

    // UDP: test with `ffplay -f h264 -i udp://127.0.0.1:5555`
    let mut sink: Box<dyn FrameSink> = Box::new(UdpSink::bind("127.0.0.1:5555")?);
    sink.send(&headers);

    state.renderer = Some(renderer);
    state.renderbuffer = Some(renderbuffer);
    state.damage_tracker = Some(damage_tracker);
    state.encoder = Some(encoder);
    state.frame_sink = Some(sink);

    // ── 60 fps render timer ───────────────────────────────────────────────────
    event_loop
        .handle()
        .insert_source(Timer::immediate(), move |deadline, _, state: &mut Wado| {
            if let Err(e) = render_tick(state, &output) {
                eprintln!("render_tick error: {e}");
            }
            TimeoutAction::ToInstant(deadline + Duration::from_nanos(1_000_000_000 / FPS as u64))
        })?;

    Ok(())
}

fn render_tick(state: &mut Wado, output: &Output) -> Result<(), Box<dyn std::error::Error>> {
    let renderer = state.renderer.as_mut().unwrap();
    let renderbuffer = state.renderbuffer.as_mut().unwrap();
    let damage_tracker = state.damage_tracker.as_mut().unwrap();

    let mut fb = renderer.bind(renderbuffer)?;

    smithay::desktop::space::render_output::<
        _,
        WaylandSurfaceRenderElement<GlesRenderer>,
        _,
        _,
    >(
        output,
        renderer,
        &mut fb,
        1.0,
        0,
        [&state.space],
        &[],
        damage_tracker,
        [0.1, 0.1, 0.1, 1.0],
    )?;

    state.space.elements().for_each(|window| {
        window.send_frame(
            output,
            state.start_time.elapsed(),
            Some(Duration::ZERO),
            |_, _| Some(output.clone()),
        )
    });
    state.space.refresh();
    state.popups.cleanup();
    let _ = state.display_handle.flush_clients();

    // ExportMem capture → encode → send
    let buf_size: Size<i32, Buffer> = (WIDTH as i32, HEIGHT as i32).into();
    let pixels = capture_frame(renderer, &fb, buf_size)?;

    let encoder = state.encoder.as_mut().unwrap();
    if let Some(nal_bytes) = encoder.encode_abgr(&pixels) {
        if let Some(sink) = state.frame_sink.as_mut() {
            sink.send(&nal_bytes);
        }
    }

    Ok(())
}
