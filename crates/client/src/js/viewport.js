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
