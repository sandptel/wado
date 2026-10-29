// SPDX-License-Identifier: AGPL-3.0-only
//! `--screenshot DIR`: every main screen rendered headless with Slint's software renderer and
//! written as PNG — no display, no GPU, no window on anyone's desktop. Time is this module's to
//! give: it is advanced until every entrance animation has settled, then the frame is drawn.
//!
//! `--fixture` points the daemon's library at a made-up computer (`caps::fixture`), so the
//! screens show a populated pool without touching this computer's real files.

use std::{
    cell::Cell,
    path::{Path, PathBuf},
    rc::Rc,
    time::Duration,
};

use slint::{
    ComponentHandle, PhysicalSize,
    platform::{
        Platform, WindowAdapter, WindowEvent,
        software_renderer::{MinimalSoftwareWindow, RepaintBufferType},
    },
};

use crate::{App, AppWindow, Client, Host, app, caps, client, host};

struct Shot {
    window: Rc<MinimalSoftwareWindow>,
    clock: Rc<Cell<Duration>>,
}

impl Platform for Shot {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
        Ok(self.window.clone())
    }
    fn duration_since_start(&self) -> Duration {
        self.clock.get()
    }
}

pub fn run(dir: &str, size: &str, mut opts: app::Options, fixture: bool) -> i32 {
    let out = PathBuf::from(dir);
    if let Err(e) = std::fs::create_dir_all(&out) {
        eprintln!("{}: {e}", out.display());
        return 1;
    }
    let _fx = if fixture {
        match caps::fixture::install(&out.join(".fixture")) {
            Ok(f) => Some(f),
            Err(e) => {
                eprintln!("fixture: {e}");
                return 1;
            }
        }
    } else {
        None
    };
    let (w, h) = size
        .split_once('x')
        .and_then(|(w, h)| Some((w.parse().ok()?, h.parse().ok()?)))
        .unwrap_or((1280u32, 820u32));
    let scale: f32 = std::env::var("WADO_SHOT_SCALE")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1.0);

    let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    let clock = Rc::new(Cell::new(Duration::ZERO));
    if slint::platform::set_platform(Box::new(Shot {
        window: window.clone(),
        clock: clock.clone(),
    }))
    .is_err()
    {
        eprintln!("a platform was already set");
        return 1;
    }
    let ui = match AppWindow::new() {
        Ok(u) => u,
        Err(e) => {
            eprintln!("{e}");
            return 1;
        }
    };
    opts.live = false;
    app::setup(&ui, &opts);
    host::install(&ui, false);
    client::install(&ui, false);
    window.dispatch_event(WindowEvent::ScaleFactorChanged {
        scale_factor: scale,
    });
    let phys = PhysicalSize::new((w as f32 * scale) as u32, (h as f32 * scale) as u32);
    window.set_size(phys);
    let _ = ui.show();
    host::refresh_now(&ui);
    client::refresh_now(&ui);
    let a = ui.global::<App>();
    let shoot = |name: &str| {
        // The first draw instantiates what the change revealed (and starts its entrance);
        // the second, after time has passed, is the settled frame.
        render(&window, phys, None);
        settle(&clock);
        render(&window, phys, Some(&out.join(format!("{name}.png"))));
        println!("{}", out.join(format!("{name}.png")).display());
    };
    let host_page = |p: i32| {
        a.set_mode(0);
        a.set_host_page(p);
        host::refresh_now(&ui);
    };

    host_page(0);
    shoot("01-host-overview");
    host_page(1);
    shoot("02-host-devices");
    host_page(2);
    host::refresh_now(&ui);
    shoot("03-host-pair");
    host_page(3);
    shoot("05-host-sessions");
    host_page(4);
    shoot("06-host-logs");
    host_page(5);
    ui.global::<Host>().invoke_pick_section(2);
    shoot("07-host-settings-stream");
    let hs = ui.global::<Host>();
    hs.invoke_edit("stream.max-fps".into(), "90".into());
    hs.invoke_edit("stream.max-bitrate".into(), "lots".into());
    shoot("08-host-settings-staged");
    hs.invoke_discard();
    hs.invoke_pick_section(5);
    shoot("09-host-settings-files");

    a.set_mode(1);
    a.set_client_page(0);
    client::refresh_now(&ui);
    shoot("10-client-computers");
    let c = ui.global::<Client>();
    c.invoke_open(if fixture { 1 } else { 0 });
    shoot("11-client-computer");
    c.invoke_open(if fixture { 2 } else { 0 });
    shoot("12-client-computer-offline");

    // The check words interrupt whatever page is open.
    if fixture {
        caps::fixture::ask("Galaxy Tab · Chrome", "desert badger torch shell");
        host_page(0);
        shoot("04-host-check-words");
        // Answer it so the next screens are clear of it.
        ui.global::<Host>().invoke_answer(
            caps::pairing::asking()
                .first()
                .map(|a| a.typed.clone())
                .unwrap_or_default()
                .into(),
            false,
        );
        host::refresh_now(&ui);
    }

    // Appearance: the popover, then a light scheme.
    host_page(0);
    ui.invoke_toggle_appearance();
    shoot("13-appearance");
    ui.invoke_toggle_appearance();
    a.invoke_pick_scheme("catppuccin-latte".into());
    host_page(0);
    shoot("14-host-overview-light");
    host_page(1);
    shoot("15-host-devices-light");
    a.set_mode(1);
    a.set_client_page(0);
    c.invoke_open(if fixture { 1 } else { 0 });
    shoot("16-client-computer-light");
    a.invoke_pick_scheme(
        opts.scheme
            .clone()
            .unwrap_or_else(|| crate::theme::DEFAULT.into())
            .into(),
    );

    // A minted code is withdrawn: a screenshot must not leave a live way in.
    host::withdraw_shown();
    0
}

/// Let every entrance, stagger and pop finish.
fn settle(clock: &Rc<Cell<Duration>>) {
    for _ in 0..40 {
        clock.set(clock.get() + Duration::from_millis(60));
        slint::platform::update_timers_and_animations();
    }
}

fn render(window: &Rc<MinimalSoftwareWindow>, size: PhysicalSize, path: Option<&Path>) {
    let (w, h) = (size.width as usize, size.height as usize);
    let mut buf = vec![slint::Rgb8Pixel::default(); w * h];
    window.request_redraw();
    window.draw_if_needed(|r| {
        r.render(&mut buf, w);
    });
    let Some(path) = path else { return };
    let bytes: Vec<u8> = buf.iter().flat_map(|p| [p.r, p.g, p.b]).collect();
    if let Err(e) = write_png(path, w as u32, h as u32, &bytes) {
        eprintln!("{}: {e}", path.display());
    }
}

fn write_png(path: &Path, w: u32, h: u32, rgb: &[u8]) -> Result<(), String> {
    let f = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let mut enc = png::Encoder::new(std::io::BufWriter::new(f), w, h);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    let mut wr = enc.write_header().map_err(|e| e.to_string())?;
    wr.write_image_data(rgb).map_err(|e| e.to_string())
}
