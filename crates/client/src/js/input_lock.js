// wado bridge — pointer lock, for games that want the mouse rather than a cursor.
//
// The problem it solves is not "the pointer is visible". It is that every other pointer event
// wado sends carries a *position* normalised against the video rect, and a position stops
// changing the moment the pointer reaches the edge of that rect. In a first-person game the
// camera stops turning while the real mouse keeps moving — and one more push in the same
// direction leaves the page entirely, clicking on whatever is behind it.
//
// Locked, the browser stops reporting positions and starts reporting movement, the pointer has
// no edge to reach, and `movementX/Y` keeps arriving for as long as the mouse keeps going.
//
// **Escape is not ours to intercept.** The browser exits pointer lock on Escape and, on some,
// only after a long press; it is a security guarantee and cannot be turned off. That is also
// the answer to "how do I get out of it" — there is no state here that can strand anyone, and
// `pointerlockchange` is the single source of truth, so a lock the browser dropped on its own
// (tab switch, fullscreen exit, Escape) turns the button off exactly as a second tap would.

W.pointerLock = {
  on: false,

  // Must be called inside a user gesture — a click or a tap. Browsers reject a request that
  // comes from a timer or a promise callback, which is what `bridge::call`'s eval would be if
  // this ran through the usual deferred path.
  request() {
    const video = W.videoEl || document.getElementById("wado-video");
    if (!video || !video.requestPointerLock) {
      // iPhone Safari has no pointer lock at all, and iPadOS only with a trackpad attached.
      // Saying so beats a button that silently does nothing.
      emit({ type: "status", text: "this browser cannot take the mouse" });
      return false;
    }
    try {
      // `unadjustedMovement` asks the browser for raw deltas with no OS pointer acceleration,
      // which is what a game wants — it applies its own sensitivity curve. Chromium returns a
      // promise and rejects when unsupported; Firefox and Safari ignore the argument and lock
      // anyway, so a rejection is never fatal.
      const r = video.requestPointerLock({ unadjustedMovement: true });
      if (r && r.catch) r.catch(() => { try { video.requestPointerLock(); } catch (_) {} });
    } catch (_) {
      try { video.requestPointerLock(); } catch (_) {}
    }
    return true;
  },

  release() {
    try { document.exitPointerLock(); } catch (_) {}
  },

  toggle() {
    if (this.on) this.release(); else this.request();
  },

};

// Requested from a real DOM click rather than from the button's Dioxus handler, and that is
// load-bearing: `bridge::call` is `spawn(async { eval().await })`, so by the time it ran the
// gesture would be over. A capture-phase listener runs synchronously inside it. Delegated to
// the document so it survives the button being re-rendered.
document.addEventListener("click", (e) => {
  if (e.target && e.target.closest && e.target.closest("#wado-lock")) W.pointerLock.toggle();
}, true);

document.addEventListener("pointerlockchange", () => {
  const video = W.videoEl || document.getElementById("wado-video");
  W.pointerLock.on = document.pointerLockElement === video;
  // Deltas and positions are two different conversations with the compositor; a queued
  // absolute motion flushed after the switch would throw the pointer back across the screen.
  W.coalesce.clear();
  emit({ type: "pointer_lock", on: W.pointerLock.on });
});
// A failed request is silent otherwise — the button would latch on with nothing behind it.
document.addEventListener("pointerlockerror", () => {
  W.pointerLock.on = false;
  emit({ type: "pointer_lock", on: false });
});
