// wado bridge — screen wake lock for the duration of a session.
//
// A streaming session has no touch input for long stretches, so the phone dims and sleeps
// mid-use and the session is torn down by the resulting pagehide. The lock is released by the
// browser whenever the tab is hidden and is NOT restored on return, which is why re-acquiring
// on visibilitychange is part of the feature rather than belt-and-braces.

W.wake = {
  lock: null,

  async acquire() {
    if (!navigator.wakeLock || W.wake.lock) return;
    try {
      W.wake.lock = await navigator.wakeLock.request("screen");
      W.wake.lock.addEventListener("release", () => { W.wake.lock = null; });
    } catch (_) {} // refused, or not supported — the session is still fine
  },

  release() {
    const l = W.wake.lock;
    W.wake.lock = null;
    if (l) { try { l.release(); } catch (_) {} }
  },
};

document.addEventListener("visibilitychange", () => {
  if (document.visibilityState === "visible" && W.sessionOn) W.wake.acquire();
});
