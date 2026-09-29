// wado bridge — the window switcher dial (M-P S2c, second design).
//
// A floating pill holding one icon per window in strip order. The focused window sits large at
// the centre; its neighbours shrink and fade either side, and only an odd number are visible
// at once (3 or 5) so there is always a middle. Drag it to scroll; let go and it springs —
// with a little overshoot — onto the nearest window, which becomes the focused one. Tap an
// icon to spring straight to it. The centred window's title shows while dragging.
//
// Orientation (vertical/horizontal), anchor (corner or edge) and visible count are settings;
// the default is vertical at bottom-right. It floats over the picture rather than taking a
// strip of the screen, so the stream is never letterboxed by it.
//
// While a finger is on it, and while it springs after one, the dial also *drives the strip*:
// every frame sends its position (InputEvent::StripView), and the compositor lays the real
// windows out side by side, gaps included, at exactly that point — so the windows slide and
// bounce with the dial. Once settled it hands the view back to focus-following, which by then
// is the same place.
//
// Built by hand into a JS-owned mount, like the gamepad: it redraws every animation frame
// while moving, which must not go through a Dioxus re-render. The physics and layout are pure
// functions on W.dialMath so scripts/switcher-check.mjs can pin them without a DOM.

W.dialMath = {
  // Pixels between neighbouring icon centres along the dial.
  SPACING: 46,
  // Spring constants, per millisecond. Damping ratio ≈ 0.37: under-damped on purpose, so a
  // settle overshoots once and comes back — the bounce is the feedback that it snapped.
  K: 0.0009,
  C: 0.022,

  // Look of an icon `d` slots from the centre when `half` slots show either side.
  item(d, half) {
    const a = Math.abs(d);
    return {
      offset: d * W.dialMath.SPACING,
      scale: Math.max(0.35, 1 - 0.28 * a),
      // Fades out over the last half-slot instead of popping at the edge.
      opacity: Math.max(0, Math.min(1, half + 0.5 - a)) * Math.max(0.35, 1 - 0.3 * a),
    };
  },

  // Past either end the dial gives, but less and less: a rubber band, not a wall.
  rubber(p, n) {
    const max = Math.max(0, n - 1);
    if (p < 0) return p * 0.35;
    if (p > max) return max + (p - max) * 0.35;
    return p;
  },

  // Where a release at position `p` with velocity `v` (slots per ms) comes to rest. A flick
  // carries on for ~150 ms of its speed before rounding, so a fast swipe skips windows.
  settle(p, v, n) {
    const projected = p + v * 150;
    return Math.max(0, Math.min(n - 1, Math.round(projected)));
  },

  // One semi-implicit Euler step of the spring towards `target`.
  step(p, v, target, dt) {
    const a = -W.dialMath.K * (p - target) - W.dialMath.C * v;
    const nv = v + a * dt;
    return [p + nv * dt, nv];
  },
};

W.dial = {
  orient: "vertical",
  pos: "bottom-right",
  count: 3,
  p: 0,           // current position, in slots (float)
  v: 0,           // velocity, slots per ms
  target: 0,
  raf: null,
  drag: null,
  live: false,    // driving the compositor's view (a gesture, not an outside focus change)
  els: [],        // icon elements, strip order
  ids: "",        // the id list the elements were built for

  // Settings setter, from the Rust UI (see ui/live.rs).
  configure(orient, pos, count) {
    this.orient = orient === "horizontal" ? "horizontal" : "vertical";
    this.pos = pos || "bottom-right";
    this.count = +count === 5 ? 5 : 3;
    this.ids = ""; // force a rebuild
    this.render(W.windows);
  },

  root() { return document.getElementById("wado-dial-mount"); },

  render(list) {
    const mount = this.root();
    if (!mount) return;
    const show = W.sessionOn && list.length > 0;
    mount.classList.toggle("on", show);
    if (!show) { mount.replaceChildren(); this.els = []; this.ids = ""; return; }
    if (!W.appsList && !W._appsAsked) { W._appsAsked = true; W.requestApps(); }

    const ids = list.map((w) => w.id + ":" + (w.app_id || "") + ":" + !!iconFor(w.app_id)).join(",");
    if (ids !== this.ids) this.build(mount, list, ids);
    this.els.forEach((el, i) => {
      el.title = list[i].title || list[i].app_id || "window";
      el.setAttribute("aria-label", el.title);
    });
    // Follow focus that moved elsewhere (a tap in the app, a launch) — but never during our own
    // gesture: a list update landing mid-spring (a title change) still names the old focus,
    // and would yank the spring back to it.
    const fi = Math.max(0, list.findIndex((w) => w.focused));
    if (!this.drag && !this.live) this.springTo(fi, false);
  },

  build(mount, list, ids) {
    this.ids = ids;
    const pill = document.createElement("div");
    pill.className = `dial ${this.orient} at-${this.pos}`;
    const half = (this.count - 1) / 2;
    pill.style.setProperty("--dial-len", `${(half * 2 + 1) * W.dialMath.SPACING + 12}px`);
    const bubble = document.createElement("div");
    bubble.className = "dialtitle";
    pill.appendChild(bubble);
    this.els = list.map((w) => {
      const el = document.createElement("div");
      el.className = "dialicon";
      const src = iconFor(w.app_id);
      if (src) {
        const img = document.createElement("img");
        img.src = src;
        img.alt = "";
        el.appendChild(img);
      } else {
        el.textContent = (w.title || w.app_id || "?").trim().charAt(0).toUpperCase() || "?";
      }
      pill.appendChild(el);
      return el;
    });
    this.bubble = bubble;
    mount.replaceChildren(pill);
    this.wire(pill);
    this.paint();
  },

  // Place every icon for the current position.
  paint() {
    const half = (this.count - 1) / 2;
    const axis = this.orient === "vertical" ? "Y" : "X";
    const centre = Math.round(Math.max(0, Math.min(this.els.length - 1, this.p)));
    this.els.forEach((el, i) => {
      const s = W.dialMath.item(i - this.p, half);
      el.style.transform = `translate${axis}(${s.offset}px) scale(${s.scale})`;
      el.style.opacity = s.opacity;
      el.style.pointerEvents = s.opacity > 0.05 ? "auto" : "none";
      el.classList.toggle("centre", i === centre);
    });
    // Coalesced per frame on the reliable channel: a late position arriving after the
    // hand-back below would leave the view stuck mid-slide, so these must stay ordered.
    if (this.live) W.coalesce.queue("strip_view", { t: "strip_view", pos: this.p });
    if (this.bubble && W.windows[centre]) {
      this.bubble.textContent = W.windows[centre].title || W.windows[centre].app_id || "";
    }
  },

  springTo(index, commit) {
    this.target = index;
    if (commit) {
      this.live = true;
      const w = W.windows[index];
      if (w && !w.focused) W.focusWindow(w.id);
    }
    if (this.raf == null) {
      let last = performance.now();
      const tick = (now) => {
        // Fixed 4 ms substeps: the spring is stable at any frame rate, 30 Hz phones included.
        let dt = Math.min(64, now - last);
        last = now;
        while (dt > 0) {
          const h = Math.min(4, dt);
          [this.p, this.v] = W.dialMath.step(this.p, this.v, this.target, h);
          dt -= h;
        }
        if (Math.abs(this.p - this.target) < 0.002 && Math.abs(this.v) < 0.0005) {
          this.p = this.target;
          this.v = 0;
          this.raf = null;
          this.paint();
          if (this.live) {
            this.live = false;
            W.coalesce.now({ t: "strip_view", pos: null });
          }
          return;
        }
        this.paint();
        this.raf = requestAnimationFrame(tick);
      };
      this.raf = requestAnimationFrame(tick);
    }
  },

  wire(pill) {
    const along = (e) => (this.orient === "vertical" ? e.clientY : e.clientX);
    pill.addEventListener("pointerdown", (e) => {
      e.preventDefault();
      e.stopPropagation();
      try { pill.setPointerCapture(e.pointerId); } catch (_) {}
      if (this.raf != null) { cancelAnimationFrame(this.raf); this.raf = null; }
      this.live = true;
      this.drag = { id: e.pointerId, a0: along(e), p0: this.p, last: along(e),
        t: performance.now(), v: 0, moved: false, icon: e.target.closest(".dialicon") };
      pill.classList.add("dragging");
    });
    pill.addEventListener("pointermove", (e) => {
      const d = this.drag;
      if (!d || e.pointerId !== d.id) return;
      const a = along(e);
      if (Math.abs(a - d.a0) > 6) d.moved = true;
      const now = performance.now();
      // Content follows the finger: dragging towards the end brings later windows to centre.
      const vNow = -(a - d.last) / W.dialMath.SPACING / Math.max(1, now - d.t);
      d.v = d.v * 0.6 + vNow * 0.4; // smoothed, so one jittery sample cannot throw a flick
      d.last = a;
      d.t = now;
      this.p = W.dialMath.rubber(d.p0 - (a - d.a0) / W.dialMath.SPACING, this.els.length);
      this.paint();
    });
    const end = (e) => {
      const d = this.drag;
      if (!d || e.pointerId !== d.id) return;
      this.drag = null;
      pill.classList.remove("dragging");
      if (!d.moved) {
        const i = d.icon ? this.els.indexOf(d.icon) : -1;
        // A tap on nothing still ends the gesture: spring home so the view is handed back.
        this.v = 0;
        this.springTo(i >= 0 ? i : Math.round(this.p), true);
        return;
      }
      this.v = d.v;
      this.springTo(W.dialMath.settle(this.p, d.v, this.els.length), true);
    };
    pill.addEventListener("pointerup", end);
    pill.addEventListener("pointercancel", end);
  },

  // The session is gone: nothing on the dial names a live window any more.
  clear() {
    W.windows = [];
    this.render([]);
  },
};

// The app_id → icon join, from the drawer's app list. See AppEntry::app_ids.
const iconFor = (appId) => {
  if (!appId || !W.appsList) return null;
  const a = W.appsList.find((e) => (e.app_ids || []).includes(appId));
  return (a && a.icon) || null;
};

W.onWindows = (list) => W.dial.render(list);
W.setSwitcher = (orient, pos, count) => W.dial.configure(orient, pos, count);
