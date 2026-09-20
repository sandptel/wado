// wado bridge — how the page occupies the screen: fullscreen, and orientation.
//
// Both exist for the same reason and only make sense together. On a phone the browser chrome
// costs a third of a portrait viewport, and a device rotating mid-session re-letterboxes a
// stream whose output size was fixed at Start and cannot be changed (invariant #8). So
// entering fullscreen also pins the orientation to the session's aspect.
//
// Every call here is best-effort. Orientation lock is unavailable on iOS Safari and on
// desktop, and fullscreen can be refused outright; a refusal must not break the session, so
// nothing throws upward.

W.isFullscreen = () => !!document.fullscreenElement;

// Is this a phone, as opposed to a desktop or a tablet?
//
// Two tests, because either alone is wrong: a coarse pointer alone also matches a touchscreen
// laptop and a TV, and a small screen alone also matches a narrow desktop window. `screen`
// rather than `window` for the size, so a half-width browser window on a desktop is not read
// as a handset. 820 CSS px on the short edge is the usual phone/tablet line — an iPad mini is
// 744, a 12.9" iPad 1024, and no phone is above 500.
//
// This is a *display* question, not an input one: the answer decides which way round the
// session is offered, and nothing else.
W.isPhone = () => {
  try {
    return (
      matchMedia("(pointer: coarse)").matches &&
      Math.min(screen.width, screen.height) <= 820
    );
  } catch (_) {
    return false;
  }
};

// Which way round a session is offered. "auto" is the historical behaviour — landscape on a
// phone, native on anything else. Set from the Rust settings panel; see `ui/session.rs`.
W.orientPref = "auto";
W.setOrientPref = (pref) => {
  W.orientPref = pref || "auto";
  W.reportScreen();
};

// ⚠️ A lock outlives the fullscreen it was taken in, and fullscreen can end without us.
//
// The gesture back, the system back button and Escape all exit fullscreen without going
// through `toggleFullscreen`, so the unlock written in its exit branch never ran — and the
// page stayed pinned to landscape in a normal browser window, with no control left on screen
// that would release it. Releasing on the *event* covers every exit, ours included.
document.addEventListener("fullscreenchange", () => {
  if (!document.fullscreenElement) {
    try { screen.orientation.unlock(); } catch (_) {}
  }
});

W.toggleFullscreen = async (w, h) => {
  try {
    if (document.fullscreenElement) {
      await document.exitFullscreen();
      // The unlock is on `fullscreenchange` above, not here: this branch is only one of the
      // ways fullscreen ends, and the others were leaving the page pinned.
      return;
    }
    // The whole shell, not just the stage: on a phone the settings sheet is a sibling of
    // the video, and fullscreening only the video would make settings unreachable until you
    // left fullscreen again.
    const el = document.querySelector(".app") || document.documentElement;
    await el.requestFullscreen({ navigationUI: "hide" });
    // Only meaningful once fullscreen is actually entered, which is why it is not a
    // standalone call: locking outside fullscreen is rejected by every browser that has it.
    if (w && h) {
      try { await screen.orientation.lock(w >= h ? "landscape" : "portrait"); } catch (_) {}
    }
  } catch (_) {}
};

// The device's physical screen, in real pixels. CSS pixels are integer-rounded, so a device
// at DPR 2.625 reports 411 rather than 411.43 and the aspect drifts — multiplying back out
// recovers the panel's true ratio, which is what a letterbox-free stream has to match.
// `screen` and not `window`: the intended mode is fullscreen, where browser chrome is gone.
W.reportScreen = () => {
  const d = window.devicePixelRatio || 1;
  let w = Math.round(screen.width * d);
  let h = Math.round(screen.height * d);
  // **Which way round the session is.** The panel is reported in whichever orientation the
  // phone happens to be held, and a phone at rest is held upright — so a session started
  // without thinking about it came out 720x1600, and every desktop application inside it then
  // had a 360px-wide screen to lay itself out on. The output's size is fixed at Start and
  // cannot be changed afterwards (invariant #8), so this is the only moment the choice exists.
  //
  // "auto" keeps that rule: landscape on a phone, native everywhere else — a desktop window is
  // already the shape its owner wants. The two explicit settings exist because a phone held
  // upright to read something is a real session, and because a desktop user testing a phone
  // layout wants the opposite. `screen.orientation.lock` in `toggleFullscreen` follows the
  // session's own aspect, so the fullscreen lock falls out of this with no second rule.
  const wantLandscape =
    W.orientPref === "landscape" ? true :
    W.orientPref === "portrait" ? false :
    W.isPhone() ? true : w >= h;
  if (wantLandscape && h > w) [w, h] = [h, w];
  if (!wantLandscape && w > h) [w, h] = [h, w];
  emit({ type: "screen", w, h, dpr: d });
};
W.reportScreen();
// Rotating swaps the axes. The running session cannot resize (invariant #8), so this only
// changes what the *next* Start will offer — no live listener beyond this.
window.addEventListener("orientationchange", () => setTimeout(W.reportScreen, 200));
