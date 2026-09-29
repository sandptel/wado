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
// Long-press an icon to act on *that* app: minimize, maximize and close spring out,
// perpendicular to the dial and well clear of the thumb that is holding it. Slide onto one and
// lift, or lift first and tap one; anything else (or a few seconds) tucks them away.
//
// Built by hand into a JS-owned mount, like the gamepad: it redraws every animation frame
// while moving, which must not go through a Dioxus re-render. The physics and layout are pure
// functions on W.dialMath so scripts/switcher-check.mjs can pin them without a DOM.

// How long a still finger on an icon takes to become a long-press.
const DIAL_HOLD_MS = 450;
// The long-press buttons: how far out from the dial, and how far apart along it.
const DIAL_ACTION_OUT = 112;
const DIAL_ACTION_GAP = 56;
const DIAL_ACTIONS = [["minimize", "—"], ["maximize", "◧"], ["close", "✕"]];

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
  actions: null,  // the open long-press buttons: { pill, index, chips, timer }
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
    // Switch the moment a window reaches the centre under the finger, not on release: the
    // dial is a selector, and waiting for the hand to lift made it feel like a preview. Only
    // while dragging — a flick's spring passes through windows it is not stopping at.
    if (this.drag && centre !== this.drag.centre) {
      this.drag.centre = centre;
      const w = W.windows[centre];
      if (w && !w.focused) W.focusWindow(w.id);
    }
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
      // A tap on an open action button: do it. Anywhere else on the dial closes them first.
      const chip = e.target.closest(".dialchip");
      if (chip) { this.fireAction(chip); return; }
      this.closeActions();
      try { pill.setPointerCapture(e.pointerId); } catch (_) {}
      if (this.raf != null) { cancelAnimationFrame(this.raf); this.raf = null; }
      this.live = true;
      this.drag = { id: e.pointerId, a0: along(e), p0: this.p, last: along(e),
        t: performance.now(), v: 0, moved: false, icon: e.target.closest(".dialicon"),
        centre: Math.round(Math.max(0, Math.min(this.els.length - 1, this.p))) };
      const d = this.drag;
      if (d.icon) d.holdTimer = setTimeout(() => this.armHold(pill, d), DIAL_HOLD_MS);
      pill.classList.add("dragging");
    });
    pill.addEventListener("pointermove", (e) => {
      const d = this.drag;
      if (!d || e.pointerId !== d.id) return;
      if (d.armed) {
        // Armed: the dial stays put; the finger may slide onto a button to pick it.
        this.hoverAction(e.clientX, e.clientY);
        return;
      }
      const a = along(e);
      if (Math.abs(a - d.a0) > 6) {
        d.moved = true;
        clearTimeout(d.holdTimer); // it is a scroll, not a hold
      }
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
      clearTimeout(d.holdTimer);
      pill.classList.remove("dragging");
      if (d.armed) {
        this.handBack();
        d.icon.classList.remove("held");
        const over = e.type !== "pointercancel" && this.actionAt(e.clientX, e.clientY);
        if (over) this.fireAction(over);
        return; // otherwise the buttons stay out for a tap
      }
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

  // The long-press fired: spring minimize / maximize / close out of the held icon.
  armHold(pill, d) {
    const i = this.els.indexOf(d.icon);
    if (i < 0 || this.drag !== d) return;
    d.armed = true;
    d.icon.classList.add("held");
    // Along the dial: centred on the icon, a button's width apart. Across it: far out towards
    // the middle of the screen, so the thumb on the icon never covers them.
    const along = W.dialMath.item(i - this.p, (this.count - 1) / 2).offset;
    const vertical = this.orient === "vertical";
    const out = DIAL_ACTION_OUT * (vertical ? (this.pos.includes("left") ? 1 : -1)
                                            : (this.pos.includes("top") ? 1 : -1));
    const chips = DIAL_ACTIONS.map(([action, glyph], k) => {
      const spread = (k - 1) * DIAL_ACTION_GAP;
      const [ax, ay] = vertical ? [0, along] : [along, 0];
      const [tx, ty] = vertical ? [out, along + spread] : [along + spread, out];
      const chip = document.createElement("div");
      chip.className = "dialchip " + action;
      chip.dataset.action = action;
      chip.textContent = glyph;
      chip.title = action;
      chip.style.setProperty("--from", `translate(${ax}px, ${ay}px) scale(0.3)`);
      chip.style.setProperty("--to", `translate(${tx}px, ${ty}px) scale(1)`);
      chip.style.animationDelay = `${k * 45}ms`; // they spring out one after another
      pill.appendChild(chip);
      return chip;
    });
    this.actions = { pill, index: i, chips, timer: setTimeout(() => this.closeActions(), 4000) };
    if (navigator.vibrate) navigator.vibrate(12);
  },

  // The button under a client point, if any.
  actionAt(x, y) {
    if (!this.actions) return null;
    return this.actions.chips.find((c) => {
      const r = c.getBoundingClientRect();
      return x >= r.left && x < r.right && y >= r.top && y < r.bottom;
    }) || null;
  },

  hoverAction(x, y) {
    if (!this.actions) return;
    const over = this.actionAt(x, y);
    this.actions.chips.forEach((c) => c.classList.toggle("hover", c === over));
  },

  // Perform a button's action on the held app, then put the buttons away.
  fireAction(chip) {
    const a = this.actions;
    if (!a) return;
    const w = W.windows[a.index];
    chip.classList.add("fired");
    this.closeActions(chip);
    if (!w) return;
    // Window actions act on the focused window, so focus the held one first; both ride the
    // same ordered socket, so the compositor sees them in this order.
    if (!w.focused) W.focusWindow(w.id);
    W.windowAction(chip.dataset.action);
  },

  // Tuck the buttons back in (all but `keep`, which is mid-"fired" animation).
  closeActions(keep) {
    const a = this.actions;
    if (!a) return;
    this.actions = null;
    clearTimeout(a.timer);
    a.chips.forEach((c) => {
      if (c !== keep) c.classList.add("retract");
      setTimeout(() => c.remove(), 240);
    });
    const held = this.els[a.index];
    if (held) held.classList.remove("held");
  },

  // The press made the dial live, and a finger's jitter before the hold may have sent a view
  // position — hand the strip back, or it stays pinned where that jitter left it.
  handBack() {
    if (this.live) {
      this.live = false;
      W.coalesce.now({ t: "strip_view", pos: null });
    }
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

// A tap anywhere off the dial puts open action buttons away (and is otherwise untouched).
document.addEventListener("pointerdown", (e) => {
  const a = W.dial.actions;
  if (a && !a.pill.contains(e.target)) W.dial.closeActions();
}, true);
