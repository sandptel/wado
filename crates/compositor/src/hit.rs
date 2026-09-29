//! What is under a point, as the accessibility query needs it.
//!
//! One job: turn a normalized output point into the window there, the process that owns it,
//! and the point in that window's own coordinates — the frame an AT-SPI tree reports extents
//! in. The query itself runs in the server (it is async D-Bus); this is the half only the
//! compositor can answer.

use smithay::reexports::wayland_server::Resource;

use crate::Wado;

/// The window under a point, for an accessibility query.
#[derive(Debug, Clone, PartialEq)]
pub struct HitWindow {
    /// The session's accessibility bus (`AT_SPI_BUS_ADDRESS`).
    pub a11y: String,
    /// The client process that owns the window — how its tree is found on the bus.
    pub pid: i32,
    /// `xdg_toplevel.title`, to pick the right frame when an app has several windows.
    pub title: String,
    /// The point, in the window's own logical pixels from its geometry origin.
    pub local: (f64, f64),
    /// Where that origin is on the output (output logical pixels), and the factor the window
    /// is drawn at (see `crate::strip`), so extents can be mapped back.
    pub origin: (f64, f64),
    pub factor: f64,
    /// The output's logical size, for normalizing.
    pub output: (f64, f64),
}

impl Wado {
    /// The window under normalized `(x, y)`, or `None` with no session, no accessibility bus,
    /// or nothing there.
    pub fn hit_window(&self, x: f64, y: f64) -> Option<HitWindow> {
        let a11y = self.app_a11y.as_ref()?.address().to_string();
        // Already mapped through any shrink, so it is in the window's own units.
        let p = self.map_point(x, y)?;
        let (window, loc) = self.space.element_under(p).map(|(w, l)| (w.clone(), l))?;
        let local = (p.x - f64::from(loc.x), p.y - f64::from(loc.y));
        self.describe(&window, a11y, local)
    }

    /// The menu (any popup) open on the focused window, as the S7 menu sheet needs it: the
    /// window, and the popup's rectangle in that window's own coordinates.
    pub fn open_menu(&self) -> Option<MenuSpot> {
        let a11y = self.app_a11y.as_ref()?.address().to_string();
        let window = self.focused_window()?;
        let (popup, offset) =
            smithay::desktop::PopupManager::popups_for_surface(window.toplevel()?.wl_surface())
                .next()?;
        let size = popup.geometry().size;
        let rect = (
            f64::from(offset.x),
            f64::from(offset.y),
            f64::from(size.w),
            f64::from(size.h),
        );
        let centre = (rect.0 + rect.2 / 2.0, rect.1 + rect.3 / 2.0);
        Some(MenuSpot {
            window: self.describe(&window, a11y, centre)?,
            rect,
        })
    }

    fn describe(
        &self,
        window: &smithay::desktop::Window,
        a11y: String,
        local: (f64, f64),
    ) -> Option<HitWindow> {
        let out = self
            .space
            .outputs()
            .next()
            .and_then(|o| self.space.output_geometry(o))?;
        let loc = self.space.element_location(window)?;
        let surface = window.toplevel()?.wl_surface();
        let pid = surface
            .client()?
            .get_credentials(&self.display_handle)
            .ok()?
            .pid;
        let title = smithay::wayland::compositor::with_states(surface, |s| {
            s.data_map
                .get::<smithay::wayland::shell::xdg::XdgToplevelSurfaceData>()
                .and_then(|d| d.lock().ok()?.title.clone())
        })
        .unwrap_or_default();
        Some(HitWindow {
            a11y,
            pid,
            title,
            local,
            origin: (f64::from(loc.x - out.loc.x), f64::from(loc.y - out.loc.y)),
            factor: self.window_scale(window),
            output: (f64::from(out.size.w), f64::from(out.size.h)),
        })
    }

    /// Publish the open menu (or its closing) when it changes — from the post-dispatch hook,
    /// like the window list, and for the same reason: popups open and close on their own.
    pub fn publish_menu(&mut self) {
        let menu = self.open_menu();
        self.menu_tx.send_if_modified(|cur| {
            if *cur == menu {
                return false;
            }
            *cur = menu;
            true
        });
    }
}

/// A popup open on the focused window — see [`Wado::open_menu`].
#[derive(Debug, Clone, PartialEq)]
pub struct MenuSpot {
    /// The window, with `local` at the popup's centre.
    pub window: HitWindow,
    /// The popup, in the window's own logical pixels: x, y, w, h.
    pub rect: (f64, f64, f64, f64),
}
