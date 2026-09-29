// wado bridge — the phone shell's bottom bar (M-P S2c).
//
// One icon per window in strip order (W.windows, from the compositor), the focused one in full
// colour and the rest tinted. Tap an icon to focus that window; swipe along the bar to move to
// the neighbouring column. Built by hand into a JS-owned mount (like the gamepad) because a
// swipe redraws on every finger movement, which must not go through a Dioxus re-render.
//
// Shown on touch devices only (`pointer: coarse`), whatever the placement: focusing a window by
// id works in every mode, and a desktop has a mouse and a taskbar-free habit already.
//
// The swipe is animated here, not by the compositor: the picture follows the finger at display
// rate, and the compositor is asked to switch only on release. Otherwise the scroll would move
// at stream latency and feel like dragging through mud. Until thumbnails land (S2d) the picture
// slides and springs back while the new column arrives.

// Fraction of the video's width a release must travel to switch columns.
const SWIPE_COMMIT = 0.2;
// A flick commits regardless of distance above this speed, in CSS px per ms.
const SWIPE_FLICK = 0.5;
// Below this travel a press on the bar is a tap on an icon, not a swipe.
const SWIPE_TAP = 10;

const coarse = () => window.matchMedia && window.matchMedia("(pointer: coarse)").matches;

// The app_id → icon join, from the drawer's app list. See AppEntry::app_ids.
const iconFor = (appId) => {
  if (!appId || !W.appsList) return null;
  const a = W.appsList.find((e) => (e.app_ids || []).includes(appId));
  return (a && a.icon) || null;
};

const tile = (w) => {
  const b = document.createElement("button");
  b.className = "stripicon" + (w.focused ? " focused" : "");
  b.title = w.title || w.app_id || "window";
  b.setAttribute("aria-label", b.title);
  const src = iconFor(w.app_id);
  if (src) {
    const img = document.createElement("img");
    img.src = src;
    img.alt = "";
    b.appendChild(img);
  } else {
    // Same fallback as the drawer: the first letter, so an iconless app is still tellable apart.
    b.textContent = (w.title || w.app_id || "?").trim().charAt(0).toUpperCase() || "?";
  }
  b.dataset.id = String(w.id);
  return b;
};

W.stripBar = {
  render(list) {
    const mount = document.getElementById("wado-strip-mount");
    if (!mount) return;
    const show = coarse() && W.sessionOn && list.length > 0;
    mount.classList.toggle("on", show);
    mount.replaceChildren(...(show ? list.map(tile) : []));
    // The icons need the app list; ask once if the drawer has not already fetched it.
    if (show && !W.appsList && !W._appsAsked) {
      W._appsAsked = true;
      W.requestApps();
    }
  },

  // The session is gone: nothing on the bar names a live window any more.
  clear() {
    W.windows = [];
    W.stripBar.render([]);
  },
};
W.onWindows = (list) => W.stripBar.render(list);

// ── Swipe ────────────────────────────────────────────────────────────────────

let swipe = null;

const slide = (dx, animate) => {
  const v = W.videoEl;
  if (!v) return;
  v.style.transition = animate ? "transform 160ms ease-out" : "none";
  v.style.transform = dx ? `translateX(${dx}px)` : "";
};

// The neighbour `dir` (+1 right, -1 left) of the focused window in strip order, if any.
const neighbour = (dir) => {
  const i = W.windows.findIndex((w) => w.focused);
  return W.windows[(i < 0 ? 0 : i) + dir] || null;
};

(function wire() {
  const mount = document.getElementById("wado-strip-mount");
  if (!mount) { requestAnimationFrame(wire); return; }
  mount.addEventListener("pointerdown", (e) => {
    try { mount.setPointerCapture(e.pointerId); } catch (_) {}
    swipe = { id: e.pointerId, x0: e.clientX, t0: performance.now(), dx: 0,
      target: e.target.closest(".stripicon") };
  });
  mount.addEventListener("pointermove", (e) => {
    if (!swipe || e.pointerId !== swipe.id) return;
    swipe.dx = e.clientX - swipe.x0;
    if (Math.abs(swipe.dx) < SWIPE_TAP) return;
    // Content follows the finger: dragging left reveals the column to the right. At either end
    // of the strip the picture only gives a little, so there is nowhere to fall off to.
    const edge = !neighbour(swipe.dx < 0 ? 1 : -1);
    slide(edge ? swipe.dx / 4 : swipe.dx, false);
  });
  const end = (e) => {
    if (!swipe || e.pointerId !== swipe.id) return;
    const { dx, t0, target } = swipe;
    swipe = null;
    if (Math.abs(dx) < SWIPE_TAP) {
      if (target) W.focusWindow(Number(target.dataset.id));
      return;
    }
    const width = (W.videoEl && W.videoEl.getBoundingClientRect().width) || window.innerWidth;
    const fast = Math.abs(dx) / Math.max(1, performance.now() - t0) > SWIPE_FLICK;
    const next = (Math.abs(dx) > width * SWIPE_COMMIT || fast) && neighbour(dx < 0 ? 1 : -1);
    if (next) W.focusWindow(next.id);
    slide(0, true);
  };
  mount.addEventListener("pointerup", end);
  mount.addEventListener("pointercancel", end);
})();
