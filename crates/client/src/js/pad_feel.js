// wado bridge — the trackpad's touch feedback: what the hand sees and feels, no part of what the
// laptop receives (js/pad_touch.js owns that, untouched). Draws the finger rings, the pinch
// connector, the multi-finger count chip and the ambient surface highlight, entirely from
// `transform`/`opacity` so it never competes with video decode or the send path.
//
// Tracking is always 1:1 and unesed — a ring's position is the pointer's position, this frame,
// full stop; only landing, lifting and the group overlays (connector/chip/ambient) animate.
// Pointer events can arrive faster than paint, so positions are written to a pending map and
// flushed to the DOM once per `requestAnimationFrame`.
//
//   W.padFeel.attach(el) / detach()

(() => {
  const F = (W.padFeel = {});
  const SENTINEL_LIFT_MS = 200; // animationend fallback for the release ripple
  const HALO_FADE_MS = 140;

  let el = null;
  const fingers = new Map(); // pointerId → {ring, halo, x, y}
  let raf = 0;
  let connEl = null;
  let midEl = null;
  let chipEl = null;

  function reduced() {
    const m = document.documentElement.getAttribute("data-motion");
    return m === "reduced" || m === "off" ||
      (typeof matchMedia === "function" && matchMedia("(prefers-reduced-motion: reduce)").matches);
  }

  // In the pad's own frame (js/pad_orient.js): the rings are children of the pad, so when it is
  // drawn turned they must be placed in its turned coordinates, not the screen's.
  function place(f, e) {
    const p = W.padOrient.toPad(e.clientX, e.clientY, el.getBoundingClientRect());
    f.x = p.x;
    f.y = p.y;
  }

  function schedule() {
    if (!raf) raf = requestAnimationFrame(paint);
  }

  function paint() {
    raf = 0;
    if (!el) return;
    for (const f of fingers.values()) {
      const t = `translate(${f.x}px, ${f.y}px)`;
      f.ring.style.transform = t;
      if (f.halo) f.halo.style.transform = t;
    }
    paintGroup();
  }

  function paintGroup() {
    const pts = [...fingers.values()];
    if (pts.length) {
      const cx = pts.reduce((s, f) => s + f.x, 0) / pts.length;
      const cy = pts.reduce((s, f) => s + f.y, 0) / pts.length;
      el.style.setProperty("--tx", `${cx}px`);
      el.style.setProperty("--ty", `${cy}px`);
    }
    if (pts.length === 2) showConnector(pts[0], pts[1]); else hideConnector();
    if (pts.length >= 3) showChip(pts.length, pts); else hideChip();
  }

  function ensureConn() {
    if (connEl) return;
    connEl = document.createElement("div");
    connEl.className = "tpconn";
    midEl = document.createElement("div");
    midEl.className = "tpmid";
    el.appendChild(connEl);
    el.appendChild(midEl);
  }
  function showConnector(a, b) {
    ensureConn();
    const dx = b.x - a.x, dy = b.y - a.y;
    const dist = Math.hypot(dx, dy);
    const angle = (Math.atan2(dy, dx) * 180) / Math.PI;
    connEl.style.transform = `translate(${a.x}px, ${a.y}px) rotate(${angle}deg) scaleX(${dist})`;
    midEl.style.transform = `translate(${(a.x + b.x) / 2}px, ${(a.y + b.y) / 2}px)`;
    if (!connEl.classList.contains("show")) {
      const t = reduced() ? "none" : "opacity 90ms var(--ease)";
      connEl.style.transition = t;
      midEl.style.transition = t;
      connEl.classList.add("show");
      midEl.classList.add("show");
    }
  }
  function hideConnector() {
    if (!connEl || !connEl.classList.contains("show")) return;
    const t = reduced() ? "none" : "opacity 120ms var(--ease)";
    connEl.style.transition = t;
    midEl.style.transition = t;
    connEl.classList.remove("show");
    midEl.classList.remove("show");
  }

  function ensureChip() {
    if (chipEl) return;
    chipEl = document.createElement("div");
    chipEl.className = "tpchip";
    el.appendChild(chipEl);
  }
  function showChip(n, pts) {
    ensureChip();
    const cx = pts.reduce((s, f) => s + f.x, 0) / pts.length;
    const cy = pts.reduce((s, f) => s + f.y, 0) / pts.length;
    chipEl.textContent = String(n);
    chipEl.style.transform = `translate(${cx}px, ${cy}px)`;
    if (!chipEl.classList.contains("show")) {
      chipEl.style.transition = reduced() ? "none" : "opacity 100ms var(--ease)";
      chipEl.classList.add("show");
    }
  }
  function hideChip() {
    if (!chipEl || !chipEl.classList.contains("show")) return;
    chipEl.style.transition = reduced() ? "none" : "opacity 120ms var(--ease)";
    chipEl.classList.remove("show");
  }

  function land(e) {
    if (fingers.has(e.pointerId)) return;
    const ring = document.createElement("i");
    ring.className = "tpfinger";
    let halo = null;
    el.appendChild(ring);
    if (!reduced()) {
      halo = document.createElement("i");
      halo.className = "tphalo land";
      el.insertBefore(halo, ring);
    }
    const f = { ring, halo, x: 0, y: 0 };
    place(f, e);
    fingers.set(e.pointerId, f);
    const t = `translate(${f.x}px, ${f.y}px)`;
    ring.style.transform = t;
    if (halo) halo.style.transform = t;
    if (reduced()) {
      ring.style.opacity = "1";
    } else {
      ring.classList.add("land");
      ring.addEventListener("animationend", () => ring.classList.remove("land"), { once: true });
      halo.addEventListener("animationend", () => halo.classList.remove("land"), { once: true });
    }
    el.classList.add("tp-ambient");
    schedule();
  }

  function lift(e) {
    const f = fingers.get(e.pointerId);
    if (!f) return;
    fingers.delete(e.pointerId);
    const { ring, halo } = f;
    if (halo) {
      if (reduced()) halo.remove();
      else {
        halo.style.transition = `opacity ${HALO_FADE_MS}ms ease-in`;
        halo.style.opacity = "0";
        setTimeout(() => halo.remove(), HALO_FADE_MS + 40);
      }
    }
    if (reduced()) {
      ring.remove();
    } else {
      ring.classList.add("lift");
      const done = () => ring.remove();
      ring.addEventListener("animationend", done, { once: true });
      setTimeout(done, SENTINEL_LIFT_MS);
    }
    if (fingers.size === 0) el.classList.remove("tp-ambient");
    schedule();
  }

  function move(e) {
    const f = fingers.get(e.pointerId);
    if (!f) return;
    place(f, e);
    schedule();
  }

  const on = [["pointerdown", land], ["pointermove", move], ["pointerup", lift],
    ["pointercancel", lift], ["lostpointercapture", lift]];

  F.attach = (node) => {
    F.detach();
    el = node;
    for (const [ev, fn] of on) el.addEventListener(ev, fn);
  };

  F.detach = () => {
    if (!el) return;
    for (const [ev, fn] of on) el.removeEventListener(ev, fn);
    if (raf) { cancelAnimationFrame(raf); raf = 0; }
    for (const f of fingers.values()) { f.ring.remove(); if (f.halo) f.halo.remove(); }
    fingers.clear();
    if (connEl) { connEl.remove(); midEl.remove(); connEl = null; midEl = null; }
    if (chipEl) { chipEl.remove(); chipEl = null; }
    el.classList.remove("tp-ambient");
    el = null;
  };
})();
