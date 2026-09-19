//! Keeping windows inside the output.
//!
//! One job: an application must not open, or stay, bigger than the screen it is being streamed
//! to. Three things were missing and they failed together, which is why they are fixed
//! together:
//!
//! * `xdg_toplevel.configure_bounds` was never sent, so a client had **no idea how big the
//!   screen was** and opened at whatever size it likes on a desktop. This is the polite half:
//!   a hint, sent before the initial configure, that GTK4, Qt6 and recent Electron honour.
//! * the initial configure carried no size, so nothing followed up on the hint.
//! * `refit_windows` resized only *maximized* windows on a reconfigure and otherwise moved
//!   windows without resizing them — so raising the scale shrank the logical output underneath
//!   every window already open and none of them noticed.
//!
//! **Why this bites hardest on a phone.** Scale is a divisor on the logical output: 720p at
//! scale 2 is a 640×360 logical screen, which is smaller than the *minimum* size most desktop
//! toolkits will accept. A client is allowed to refuse a configure it cannot honour, and those
//! ones do. Nothing in this module can fix that case — the answer there is scaling the window
//! at render time, which is a milestone of its own. What this module does is make sure the
//! cases that *can* comply are asked to, which is most of them.
//!
//! ponytail: a per-axis clamp, not an aspect-preserving fit. "Fit on screen" for a window
//! manager means a maximum, and shrinking a 2000×500 window to 640×160 to preserve its shape
//! would be a worse answer than the overflow. True aspect-preserving fit belongs with the
//! render-time rescale.

use smithay::{
    desktop::Window,
    utils::{Logical, Size},
    wayland::shell::xdg::ToplevelSurface,
};

/// The size a window of `window` should be configured at to fit `output`, or `None` when it
/// already fits and must be left alone.
///
/// `None` rather than "the same size" on purpose: every caller turns it into a configure, and a
/// configure that changes nothing still makes the client re-render.
pub fn shrink_to(
    window: Size<i32, Logical>,
    output: Size<i32, Logical>,
) -> Option<Size<i32, Logical>> {
    // A window with no size yet has not committed a buffer; an output with no size is a
    // session mid-teardown. Neither is a window that is too big.
    if output.w <= 0 || output.h <= 0 || window.w <= 0 || window.h <= 0 {
        return None;
    }
    let fitted: Size<i32, Logical> = (window.w.min(output.w), window.h.min(output.h)).into();
    (fitted != window).then_some(fitted)
}

/// Tell a toplevel how big the screen is. Does **not** configure it.
///
/// Set before the initial configure is sent, because that is the one a client sizes itself
/// from; smithay diffs it against the last value, so re-advertising an unchanged bound costs
/// nothing and is the right thing to do whenever the output changes shape.
pub fn advertise_bounds(toplevel: &ToplevelSurface, output: Size<i32, Logical>) {
    if output.w <= 0 || output.h <= 0 {
        return;
    }
    toplevel.with_pending_state(|s| s.bounds = Some(output));
}

/// Configure `window` down to `output` if it is too big, and report the size it will end up
/// at — which is what a caller should place it by.
///
/// The returned size is the one *asked for*, not the one the client has committed: placing a
/// window by its old size would leave it off-centre the moment it complies, and a client that
/// refuses was going to overflow whatever we did.
pub fn fit(window: &Window, output: Size<i32, Logical>) -> Size<i32, Logical> {
    let size = window.geometry().size;
    let Some(fitted) = shrink_to(size, output) else {
        return size;
    };
    if let Some(toplevel) = window.toplevel() {
        toplevel.with_pending_state(|s| s.size = Some(fitted));
        toplevel.send_pending_configure();
    }
    fitted
}

#[cfg(test)]
mod tests {
    use super::*;

    fn size(w: i32, h: i32) -> Size<i32, Logical> {
        (w, h).into()
    }

    #[test]
    fn a_window_that_fits_is_left_alone() {
        let output = size(1280, 720);
        assert_eq!(shrink_to(size(800, 600), output), None);
        // Exactly the output is not "too big" — a maximized window hits this every reconfigure.
        assert_eq!(shrink_to(output, output), None);
    }

    #[test]
    fn each_axis_is_clamped_on_its_own() {
        let output = size(1280, 720);
        // Only one axis over: the other must not move, or a wide window gets letterboxed for
        // no reason.
        assert_eq!(shrink_to(size(1920, 600), output), Some(size(1280, 600)));
        assert_eq!(shrink_to(size(800, 1080), output), Some(size(800, 720)));
        assert_eq!(shrink_to(size(1920, 1080), output), Some(output));
    }

    #[test]
    fn the_case_a_phone_actually_hits() {
        // 720p at scale 2. A GTK app opening at its own 900x700 default is nearly twice the
        // screen in both directions.
        let output = size(640, 360);
        assert_eq!(shrink_to(size(900, 700), output), Some(output));
    }

    #[test]
    fn nothing_is_configured_from_a_size_that_is_not_real_yet() {
        // Pre-first-commit window, and a session being torn down. Both must be no-ops rather
        // than a configure to 0x0, which clients read as "you pick" and would undo the fit.
        assert_eq!(shrink_to(size(0, 0), size(1280, 720)), None);
        assert_eq!(shrink_to(size(800, 600), size(0, 0)), None);
    }
}
