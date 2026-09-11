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

W.toggleFullscreen = async (w, h) => {
  try {
    if (document.fullscreenElement) {
      await document.exitFullscreen();
      // Releasing on exit matters: a lock left behind would pin the *page* after the user
      // has gone back to a normal browser window.
      try { screen.orientation.unlock(); } catch (_) {}
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
  emit({
    type: "screen",
    w: Math.round(screen.width * d),
    h: Math.round(screen.height * d),
    dpr: d,
  });
};
W.reportScreen();
// Rotating swaps the axes. The running session cannot resize (invariant #8), so this only
// changes what the *next* Start will offer — no live listener beyond this.
window.addEventListener("orientationchange", () => setTimeout(W.reportScreen, 200));
