// wado bridge — the deck's Touchpad face (`ui/surround/deck/pad.rs`): a laptop touchpad for the
// session's own pointer.
//
//   one finger slides   → the pointer moves by how far it went (faster slides go further)
//   tap                 → left click where the pointer is
//   two-finger tap      → right click
//   two fingers slide   → scroll where the pointer is
//   Left / Right zones  → hold a button down (with a finger on the pad: drag)
//
// It drives **the session**, over the input channel the picture already uses (`pointer_motion`,
// `button`, `scroll`). Not `W.pad`: the trackpad link drives the computer's *own* desktop
// through a virtual touchpad, which is not the screen in this picture. The compositor draws no
// cursor, so this draws one over the picture (`#deck-cursor`) where it sent the pointer.
//
// Delegated from the document, so Dioxus may re-mount the face's markup freely. Nothing here
// touches the picture's own input path.
//
//   W.deckPad.refit()   the picture moved (a morph, a resize): put the cursor back over it

(() => {
  const D = (W.deckPad = {});
  const TAP_MS = 220, TAP_PX = 8;
  const pos = { x: 0.5, y: 0.5 };
  const fingers = new Map(); // pointerId → {x, y, x0, y0, t0}
  let moved = false, maxFingers = 0, scrollAt = null, cursor = null;
  const held = new Set();

  const video = () => document.getElementById("wado-video");
  function content() {
    const v = video();
    if (!v) return null;
    if (W.targets && v.videoWidth) return W.targets.content(v);
    const r = v.getBoundingClientRect();
    return r.width ? { left: r.left, top: r.top, w: r.width, h: r.height } : null;
  }

  D.refit = () => {
    const on = document.documentElement.dataset.arr === "console" && !!document.getElementById("deck-pad");
    if (!on) { if (cursor) cursor.classList.remove("in"); return; }
    const c = content();
    if (!c) return;
    if (!cursor || !cursor.isConnected) {
      cursor = document.createElement("div");
      cursor.id = "deck-cursor";
      cursor.setAttribute("aria-hidden", "true");
      document.body.appendChild(cursor);
    }
    cursor.style.translate = `${(c.left + pos.x * c.w).toFixed(1)}px ${(c.top + pos.y * c.h).toFixed(1)}px`;
    cursor.classList.add("in");
  };

  const clamp = (v) => Math.min(1, Math.max(0, v));
  function point() { return { x: pos.x, y: pos.y }; }

  function move(dx, dy, dt) {
    const c = content();
    if (!c) return;
    // A slow slide is precise; a quick one crosses the picture. Gain on the finger's speed.
    const speed = Math.hypot(dx, dy) / Math.max(1, dt);
    const gain = 1.3 + Math.min(1.7, speed * 1.1);
    pos.x = clamp(pos.x + (dx * gain) / c.w);
    pos.y = clamp(pos.y + (dy * gain) / c.h);
    W.coalesce.queue("pointer_motion", { t: "pointer_motion", ...point() });
    D.refit();
  }

  function click(button) {
    const p = point();
    W.coalesce.now({ t: "button", ...p, button, pressed: true });
    W.sendInput({ t: "button", ...p, button, pressed: false });
    if (cursor) { cursor.classList.remove("tap"); void cursor.offsetWidth; cursor.classList.add("tap"); }
  }

  const surface = (e) => e.target && e.target.closest && e.target.closest("#deck-pad");
  const zone = (e) => e.target && e.target.closest && e.target.closest("[data-dbtn]");

  document.addEventListener("pointerdown", (e) => {
    const z = zone(e);
    if (z) {
      e.preventDefault();
      const b = z.dataset.dbtn === "right" ? "right" : "left";
      held.add(b);
      z.classList.add("down");
      W.coalesce.now({ t: "button", ...point(), button: b, pressed: true });
      try { z.setPointerCapture(e.pointerId); } catch (_) {}
      return;
    }
    const s = surface(e);
    if (!s) return;
    e.preventDefault();
    try { s.setPointerCapture(e.pointerId); } catch (_) {}
    if (!fingers.size) { moved = false; maxFingers = 0; }
    fingers.set(e.pointerId, { x: e.clientX, y: e.clientY, x0: e.clientX, y0: e.clientY, t: performance.now(), t0: performance.now() });
    maxFingers = Math.max(maxFingers, fingers.size);
    D.refit();
  }, true);

  document.addEventListener("pointermove", (e) => {
    const f = fingers.get(e.pointerId);
    if (!f) return;
    const now = performance.now();
    const dx = e.clientX - f.x, dy = e.clientY - f.y, dt = now - f.t;
    f.x = e.clientX; f.y = e.clientY; f.t = now;
    if (Math.hypot(e.clientX - f.x0, e.clientY - f.y0) > TAP_PX) moved = true;
    if (!moved) return;
    if (fingers.size >= 2) {
      // Two fingers: scroll, once per pair — follow the first finger only.
      if (e.pointerId !== fingers.keys().next().value) return;
      const v = video(), c = content();
      if (!v || !c) return;
      const at = W.fingerScroll && W.fingerScroll.delta(dx, dy, c.left + pos.x * c.w, c.top + pos.y * c.h, v);
      if (at) scrollAt = at;
      return;
    }
    move(dx, dy, dt);
  }, true);

  const up = (e) => {
    const z = zone(e);
    if (z && e.type !== "pointermove") {
      const b = z.dataset.dbtn === "right" ? "right" : "left";
      z.classList.remove("down");
      if (held.delete(b)) W.coalesce.now({ t: "button", ...point(), button: b, pressed: false });
    }
    const f = fingers.get(e.pointerId);
    if (!f) return;
    fingers.delete(e.pointerId);
    if (fingers.size) return;
    if (scrollAt) { if (W.fingerScroll) W.fingerScroll.stop(scrollAt); scrollAt = null; return; }
    if (!moved && e.type === "pointerup" && performance.now() - f.t0 < TAP_MS * (maxFingers > 1 ? 1.5 : 1)) {
      click(maxFingers > 1 ? "right" : "left");
    }
  };
  document.addEventListener("pointerup", up, true);
  document.addEventListener("pointercancel", up, true);

  // A face change or the end of the session must not leave a button down on the computer.
  D.release = () => {
    for (const b of held) W.sendInput({ t: "button", ...point(), button: b, pressed: false });
    held.clear();
    fingers.clear();
  };
  addEventListener("resize", () => requestAnimationFrame(D.refit));
})();
