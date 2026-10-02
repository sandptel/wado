//! The browser-side half of the settings: everything `js/` applies itself (move mode, scroll,
//! touch, the dock pin, edge swipes, orientation, wake lock, sound) pushed to the bridge in one call.

use crate::{bridge, state::Ui};

/// Push the JS-backed Live settings to the bridge.
///
/// The counterpart to [`crate::debug::apply`], and needed for the same reason: persistence
/// restores the *signals*, but the browser-side state these mirror only ever changed inside
/// the handlers below. Without this call, a reload shows "natural scroll, 3.0x" in the panel
/// while the bridge is still scrolling at 1.0x in the other direction.
///
/// It is also the single place that knows the call shape, so the handlers cannot drift from
/// each other — `setScroll` takes both values, and updating either one has to send both.
pub fn apply(ui: Ui) {
    let s = ui.set;
    bridge::call(format!("window.__wado.setMoveMode({});", (s.move_mode)()));
    // Lives in the Session panel because it belongs beside FPS, but it is a browser-side
    // flag like the two below — so it is restored from here, not at Start.
    bridge::call(format!("window.__wado.setFpsLock({});", (s.fps_lock)()));
    bridge::call(format!(
        "window.__wado.setScroll({}, {});",
        (s.scroll_speed)(),
        (s.natural_scroll)()
    ));
    bridge::call(format!(
        "window.__wado.setTouchMode({});",
        bridge::js(&(s.touch_mode)())
    ));
    bridge::call(format!(
        "window.__wado.setDockPin({}); window.__wado.setEdgeSwipe({});",
        (s.dock_pin)(),
        (s.edge_swipe)()
    ));
    // Same shape as the two above, and for the same reason: it is a browser-side flag the
    // Rust side only stores. It re-reports the screen, so the resolution list follows it.
    bridge::call(format!(
        "window.__wado.setOrientPref({});",
        bridge::js(&(s.orientation)())
    ));
    bridge::call(format!(
        "window.__wado.setWake({}); window.__wado.setAudio({}, {});",
        (s.keep_awake)(),
        (s.volume)(),
        (s.muted)()
    ));
}
