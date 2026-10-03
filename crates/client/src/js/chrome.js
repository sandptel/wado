// wado bridge — the shell chrome's browser-side half: rotate, the pinned dock, edge swipes, and
// the window list handed to Rust (which draws the workspace bar).

// ── The window list → Rust ───────────────────────────────────────────────────
W.relayOn("windows", (msg) => {
  W.windows = msg.windows || [];
  W.workspace = msg.workspace || 1;
  emit({ type: "windows", windows: W.windows, workspace: W.workspace });
});

// ── Rotate ───────────────────────────────────────────────────────────────────
// Caught in the capture phase, inside the tap: fullscreen and the orientation lock are both
// refused outside a user gesture, and the Rust handler would run after it. Rust re-shapes the
// session (`actions::rotate`) from the event this sends.
document.addEventListener("click", async (e) => {
  const b = e.target && e.target.closest && e.target.closest("[data-rotate]");
  if (!b) return;
  const to = b.dataset.rotate === "landscape" ? "landscape" : "portrait";
  const root = document.documentElement;
  root.dataset.rotating = to;
  setTimeout(() => { delete root.dataset.rotating; }, 700);
  emit({ type: "rotate", to });
  try {
    if (!document.fullscreenElement) {
      await (document.querySelector(".app") || root).requestFullscreen({ navigationUI: "hide" });
    }
    await screen.orientation.lock(to);
  } catch (_) {
    // A desktop browser, or a phone that will not lock: the session still turns, the device
    // simply is not held to it.
  }
}, true);

// ── Pinned dock ──────────────────────────────────────────────────────────────
W.setDockPin = (on) => {
  W.bar.pinned = !!on;
  const el = document.getElementById("wado-bar");
  if (on && el) el.classList.remove("idle");
  document.querySelectorAll(".rail").forEach((r) => r.classList.toggle("pinned", !!on));
};
{
  const wake = W.bar.wake;
  W.bar.wake = () => {
    if (!W.bar.pinned) return wake();
    const el = document.getElementById("wado-bar");
    if (el) el.classList.remove("idle");
  };
}

// ── Edge swipes: from a screen edge, sideways → previous / next workspace ───
// A phone's edge gesture, on a desktop. A press that starts in the edge strip belongs to the
// gesture; the strip is narrow enough that apps lose next to nothing to it.
const EDGE = 14, SWIPE = 70;
W.edgeSwipeOn = true;
W.setEdgeSwipe = (on) => { W.edgeSwipeOn = !!on; };
W.edgeSwipe = {
  g: null,
  down(e) {
    if (!W.edgeSwipeOn || !W.sessionOn || W.activePointers.size !== 1) return false;
    const left = e.clientX < EDGE, right = e.clientX > innerWidth - EDGE;
    if (!left && !right) return false;
    this.g = { id: e.pointerId, x0: e.clientX, y0: e.clientY, side: left ? "left" : "right" };
    return true;
  },
  move(e) { return !!(this.g && this.g.id === e.pointerId); },
  up(e) {
    const g = this.g;
    if (!g || g.id !== e.pointerId) return false;
    this.g = null;
    const dx = e.clientX - g.x0, dy = e.clientY - g.y0;
    if (Math.abs(dx) > SWIPE && Math.abs(dx) > Math.abs(dy) * 1.5) {
      // Pulling in from the left is "back a workspace", as paging works on a phone.
      emit({ type: "gesture", action: dx > 0 ? "workspace-prev" : "workspace-next" });
    }
    return true;
  },
};

// ── Fullscreen, from the rail: inside the tap for the same reason as rotate ──
document.addEventListener("click", (e) => {
  if (!(e.target && e.target.closest && e.target.closest("[data-fullscreen]"))) return;
  if (document.fullscreenElement) document.exitFullscreen().catch(() => {});
  else (document.querySelector(".app") || document.documentElement)
    .requestFullscreen({ navigationUI: "hide" }).catch(() => {});
}, true);

// ── Long press on a window icon in the workspace bar → its action sheet ─────
// A timer rather than `contextmenu`: iOS never sends that for a long press. The tap that
// ends a long press is swallowed so it does not also focus the window.
{
  let hold = null, held = false;
  document.addEventListener("pointerdown", (e) => {
    const b = e.target && e.target.closest && e.target.closest("[data-win]");
    if (!b) return;
    held = false;
    clearTimeout(hold);
    hold = setTimeout(() => {
      held = true;
      if (navigator.vibrate) navigator.vibrate(12);
      emit({ type: "winMenu", id: Number(b.dataset.win) });
    }, 420);
  }, true);
  const cancel = () => clearTimeout(hold);
  document.addEventListener("pointerup", cancel, true);
  document.addEventListener("pointercancel", cancel, true);
  document.addEventListener("click", (e) => {
    if (held && e.target && e.target.closest && e.target.closest("[data-win]")) {
      held = false;
      e.stopPropagation();
      e.preventDefault();
    }
  }, true);
  document.addEventListener("contextmenu", (e) => {
    if (e.target && e.target.closest && e.target.closest("[data-win]")) e.preventDefault();
  }, true);
}

// ── Bottom sheets: drag the handle down, or tap outside, to close ────────────
// Handles carry `data-dismiss`; their own click closes the sheet, so a drag past the threshold
// only has to click it. Outside taps are swallowed: the tap that closes a sheet must not also
// land in the app under it.
{
  let d = null;
  const OUTSIDE = "#wado-bar, .rail, #cc, .ccscrim, .winsheet, .scrim";
  document.addEventListener("pointerdown", (e) => {
    const t = e.target;
    if (!t || !t.closest) return;
    const h = t.closest("[data-dismiss]");
    if (h) {
      const sheet = h.closest("section");
      if (!sheet) return;
      d = { h, sheet, id: e.pointerId, y0: e.clientY, dy: 0 };
      sheet.style.transition = "none";
      try { h.setPointerCapture(e.pointerId); } catch (_) {}
      return;
    }
    for (const id of ["drawer", "console"]) {
      const s = document.getElementById(id);
      if (!s || !s.classList.contains("open") || s.contains(t) || t.closest(OUTSIDE)) continue;
      if (id === "console" && !W.sessionOn) continue; // shell-only mode: the console is the page
      e.stopPropagation();
      e.preventDefault();
      const close = s.querySelector("[data-dismiss]");
      if (close) close.click();
      return;
    }
  }, true);
  document.addEventListener("pointermove", (e) => {
    if (!d || d.id !== e.pointerId) return;
    d.dy = Math.max(0, e.clientY - d.y0);
    d.sheet.style.transform = `translateY(${d.dy}px)`;
  }, true);
  const end = (e) => {
    if (!d || d.id !== e.pointerId) return;
    const { h, sheet, dy } = d;
    d = null;
    sheet.style.transition = "";
    sheet.style.transform = "";
    if (dy > 70) h.click();
  };
  document.addEventListener("pointerup", end, true);
  document.addEventListener("pointercancel", end, true);
}

// ── Keyboard buttons (dock, rail, quick tile) ────────────────────────────────
// Toggled here, inside the tap: Android raises the soft keyboard only for a focus made in the
// user gesture, and the Rust handler runs after it. A `<label for>` was meant to do this
// natively and did not, reliably, on the phone (2026-10-03). The toggle is on `click` — for a
// touch, the gesture is granted at pointerup — and pointerdown only keeps focus off the button,
// which would otherwise blur the field the moment it was focused.
document.addEventListener("pointerdown", (e) => {
  const b = e.target && e.target.closest && e.target.closest("[data-osk]");
  if (b && !b.disabled) e.preventDefault();
}, true);
document.addEventListener("click", (e) => {
  const b = e.target && e.target.closest && e.target.closest("[data-osk]");
  if (!b || b.disabled) return;
  W.oskToggle();
}, true);
