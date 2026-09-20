// wado bridge — the on-screen gamepad.
//
// A touch controller drawn over the video, in two modes that are genuinely different things
// rather than two settings:
//
//   "pad"   every control drives a **real virtual gamepad on the host**, created through
//           /dev/uinput by the compositor. Games that read a controller see an Xbox 360 pad.
//           This is the mode that works in a game with no keyboard support at all.
//   "keys"  every control is mapped to a **keyboard key or a mouse button** and rides the
//           input path that already exists. Nothing new is needed on the host — no device, no
//           permission — and it works in any game that reads WASD. The right stick becomes
//           mouse-look, which is the half a key map usually cannot do.
//
// Why both: the uinput device needs the daemon's user to be in the `uinput` group, which is
// not true on a fresh machine, and until it is, "keys" is the mode that works. It is also the
// only mode that can drive a game which reads the mouse for aiming.
//
// The container is pointer-events:none and only the controls themselves take input, so a tap
// in the gaps between them still reaches the video underneath.

// Linux evdev codes. Raw, because that is what the wire carries and what the kernel consumes —
// see wado_protocol::InputEvent::GamepadButton.
const BTN = {
  a: 0x130, b: 0x131, x: 0x133, y: 0x134,
  l1: 0x136, r1: 0x137,
  select: 0x13a, start: 0x13b, guide: 0x13c,
  l3: 0x13d, r3: 0x13e,
};
const ABS = { lx: 0x00, ly: 0x01, lt: 0x02, rx: 0x03, ry: 0x04, rt: 0x05, hx: 0x10, hy: 0x11 };
const STICK_MAX = 32767;
const TRIGGER_MAX = 255;

// What each control does in "keys" mode. A fixed map, not a remapper: these are the bindings
// that are near-universal in first-person games on a desktop, which is the case the mode
// exists for. `mouse` is a button; `key` is an evdev keycode (the same numbers
// input_keyboard.js sends).
//
// ponytail: no rebinding UI. A per-game remapper is a real feature with a real editor behind
// it; this is the default that makes the pad useful the first time it is switched on.
const KEYMAP = {
  a: { key: 57 },            // Space — jump
  b: { key: 29 },            // LeftCtrl — crouch
  x: { key: 18 },            // E — use / interact
  y: { key: 19 },            // R — reload
  l1: { mouse: "right" },    // aim
  r1: { mouse: "left" },     // fire
  l2: { key: 16 },           // Q
  r2: { key: 33 },           // F
  l3: { key: 42 },           // LeftShift — sprint
  r3: { key: 47 },           // V — melee
  select: { key: 15 },       // Tab — scoreboard / inventory
  start: { key: 1 },         // Escape — menu
  guide: { key: 125 },       // Super
  up: { key: 103 }, down: { key: 108 }, left: { key: 105 }, right: { key: 106 },
};
// The left stick in "keys" mode. Digital, with a deliberately large deadzone: a key is either
// down or it is not, and a small one makes a resting thumb walk.
const WASD = { up: 17, left: 30, down: 31, right: 32 };
const WALK_DEADZONE = 0.35;
// How far a full right-stick deflection turns, in logical pixels per frame. Mouse-look is a
// rate, not a position, so this is the only tuning it has.
const LOOK_SPEED = 14;

// Every control, in DOM order. `cluster` is the CSS anchor; `glyph` is what is drawn on it.
const CONTROLS = [
  { id: "l1", cluster: "shoulder-l", glyph: "L1", shape: "bumper" },
  { id: "l2", cluster: "shoulder-l", glyph: "L2", shape: "bumper" },
  { id: "r1", cluster: "shoulder-r", glyph: "R1", shape: "bumper" },
  { id: "r2", cluster: "shoulder-r", glyph: "R2", shape: "bumper" },

  { id: "select", cluster: "center", glyph: "SELECT", shape: "pill" },
  { id: "guide", cluster: "center", glyph: "⌂", shape: "pill" },
  { id: "start", cluster: "center", glyph: "START", shape: "pill" },

  { id: "up", cluster: "dpad", glyph: "▲", shape: "dpad up" },
  { id: "left", cluster: "dpad", glyph: "◀", shape: "dpad left" },
  { id: "right", cluster: "dpad", glyph: "▶", shape: "dpad right" },
  { id: "down", cluster: "dpad", glyph: "▼", shape: "dpad down" },

  { id: "y", cluster: "face", glyph: "Y", shape: "face up" },
  { id: "x", cluster: "face", glyph: "X", shape: "face left" },
  { id: "b", cluster: "face", glyph: "B", shape: "face right" },
  { id: "a", cluster: "face", glyph: "A", shape: "face down" },
];

// `layout` is the edit-mode override map: { <control id>: { x, y, s } }, where x/y are the
// control's centre as a fraction of the stage and s multiplies its size. Fractions, not
// pixels, because the same phone is a different number of pixels in a different session —
// and the pad has to land in the same place under the thumb either way.
W.padCfg = { on: false, mode: "keys", scale: 1, opacity: 0.5, insetX: 0, insetY: 0, layout: {} };

W.gamepad = {
  root: null,
  editing: false,
  selId: "",
  held: new Set(),     // control ids currently pressed, so hide() can release them all
  look: { x: 0, y: 0 },
  lookRaf: 0,

  // ── sending ──────────────────────────────────────────────────────────────────────────
  //
  // One place decides which of the two wire shapes a control produces, so a new control
  // cannot accidentally be added to one mode only.
  //
  // Everything terminal goes through `W.coalesce.now`, which flushes anything queued before
  // it sends. A press that overtook a pending stick sample would arrive before the movement
  // that led to it — and a release that did would stick the button down.
  press(id, down) {
    if (down) this.held.add(id); else this.held.delete(id);
    if (W.padCfg.mode === "pad") return this.padPress(id, down);
    const m = KEYMAP[id];
    if (!m) return;
    if (m.mouse) {
      // A gamepad button has no position, so this uses the middle of the output. Under a
      // pointer lock — which is how these games are played — position is ignored anyway, and
      // without one the centre is the least surprising place for a shot to land.
      W.coalesce.now({ t: "button", x: 0.5, y: 0.5, button: m.mouse, pressed: down });
    } else {
      W.coalesce.now({ t: "key", code: m.key, pressed: down });
    }
  },

  padPress(id, down) {
    // The D-pad is a hat on a real controller, not four buttons.
    const hat = { up: [ABS.hy, -1], down: [ABS.hy, 1], left: [ABS.hx, -1], right: [ABS.hx, 1] }[id];
    if (hat) return W.coalesce.now({ t: "gamepad_axis", code: hat[0], value: down ? hat[1] : 0 });
    // So are the lower shoulders: L2/R2 are analog triggers, and a game reading them as an
    // axis would never see a press sent as a button.
    const trig = { l2: ABS.lt, r2: ABS.rt }[id];
    if (trig !== undefined) {
      return W.coalesce.now({ t: "gamepad_axis", code: trig, value: down ? TRIGGER_MAX : 0 });
    }
    if (BTN[id] !== undefined) {
      W.coalesce.now({ t: "gamepad_button", code: BTN[id], pressed: down });
    }
  },

  // ── sticks ───────────────────────────────────────────────────────────────────────────
  //
  // `nx`/`ny` are -1..1 deflections. The left stick walks, the right stick looks; in "pad"
  // mode both are just axes.
  stick(side, nx, ny) {
    if (W.padCfg.mode === "pad") {
      const [cx, cy] = side === "l" ? [ABS.lx, ABS.ly] : [ABS.rx, ABS.ry];
      this.axis(`${side}x`, cx, Math.round(nx * STICK_MAX));
      this.axis(`${side}y`, cy, Math.round(ny * STICK_MAX));
      return;
    }
    if (side === "l") return this.walk(nx, ny);
    this.look.x = nx;
    this.look.y = ny;
    this.startLook();
  },

  // An axis is state, not an event: the far end holds the last value until a new one arrives,
  // and the kernel drops a repeated EV_ABS anyway. A thumb resting at full deflection still
  // produces a pointermove per frame, so without this the same number rides the channel sixty
  // times a second for nothing — and input is the one thing that must never be queued behind
  // anything (invariant 1).
  axis(key, code, value) {
    this._ax = this._ax || {};
    if (this._ax[key] === value) return;
    this._ax[key] = value;
    W.coalesce.queue(`pad-${key}`, { t: "gamepad_axis", code, value });
  },

  // Digital WASD with edge detection: only the keys that changed are sent, or a held stick
  // would restate every key on every pointermove and flood the reliable channel.
  walk(nx, ny) {
    const want = {
      up: ny < -WALK_DEADZONE, down: ny > WALK_DEADZONE,
      left: nx < -WALK_DEADZONE, right: nx > WALK_DEADZONE,
    };
    this._walk = this._walk || {};
    for (const dir of Object.keys(WASD)) {
      if (!!this._walk[dir] === !!want[dir]) continue;
      this._walk[dir] = want[dir];
      W.coalesce.now({ t: "key", code: WASD[dir], pressed: want[dir] });
    }
  },

  // Mouse-look runs on its own frame loop rather than on pointermove: a thumb held still at
  // full deflection produces no move events at all, and the camera would stop turning.
  startLook() {
    if (this.lookRaf) return;
    const tick = () => {
      const { x, y } = this.look;
      if (!x && !y) { this.lookRaf = 0; return; }
      // Through the coalescer, and *additively*, for the same reason input_pointer.js does:
      // this is a movement, not a position, so a frame that also carried real mouse motion
      // must sum the two rather than throw one away.
      W.coalesce.add("pointer_relative", {
        t: "pointer_relative",
        dx: x * LOOK_SPEED,
        dy: y * LOOK_SPEED,
      });
      this.lookRaf = requestAnimationFrame(tick);
    };
    this.lookRaf = requestAnimationFrame(tick);
  },

  // ── DOM ──────────────────────────────────────────────────────────────────────────────
  build() {
    if (this.root) return this.root;
    // The portal Dioxus renders for exactly this — see ui/stage.rs. Absent until the app has
    // mounted, so `apply` is safe to call before then and simply does nothing.
    const mount = document.getElementById("wado-pad-mount");
    if (!mount) return null;
    const root = document.createElement("div");
    root.id = "wado-pad";
    root.hidden = true;

    for (const c of CONTROLS) {
      const el = document.createElement("div");
      el.className = `padbtn ${c.shape}`;
      el.dataset.cluster = c.cluster;
      el.dataset.id = c.id;
      el.textContent = c.glyph;
      this.bindButton(el, c.id);
      root.appendChild(el);
    }
    for (const side of ["l", "r"]) {
      const base = document.createElement("div");
      base.className = "padstick";
      base.dataset.cluster = `stick-${side}`;
      base.dataset.id = `stick-${side}`; // the editor addresses everything by data-id
      const thumb = document.createElement("div");
      thumb.className = "padthumb";
      base.appendChild(thumb);
      this.bindStick(base, thumb, side);
      root.appendChild(base);
    }
    root.appendChild(this.buildEditBar());
    this.editBind(root);
    mount.appendChild(root);
    this.root = root;
    return root;
  },

  bindButton(el, id) {
    const down = (e) => {
      e.preventDefault();
      e.stopPropagation();
      try { el.setPointerCapture(e.pointerId); } catch (_) {}
      el.classList.add("on");
      this.press(id, true);
    };
    const up = (e) => {
      e.stopPropagation();
      if (!el.classList.contains("on")) return;
      el.classList.remove("on");
      this.press(id, false);
    };
    el.addEventListener("pointerdown", down);
    el.addEventListener("pointerup", up);
    // Both, and not just `pointerup`: a contact that leaves the element or is stolen by the
    // browser fires only one of these, and a missed release is a key held down forever.
    el.addEventListener("pointercancel", up);
    el.addEventListener("lostpointercapture", up);
  },

  bindStick(base, thumb, side) {
    let active = null;
    const moveTo = (e) => {
      const r = base.getBoundingClientRect();
      const rad = r.width / 2;
      let dx = (e.clientX - (r.left + rad)) / rad;
      let dy = (e.clientY - (r.top + rad)) / rad;
      // Clamp to the circle, not the square: a diagonal must not read as 1.41x deflection.
      const len = Math.hypot(dx, dy);
      if (len > 1) { dx /= len; dy /= len; }
      thumb.style.transform = `translate(${dx * rad * 0.55}px, ${dy * rad * 0.55}px)`;
      this.stick(side, dx, dy);
    };
    const release = (e) => {
      if (active === null) return;
      active = null;
      thumb.style.transform = "";
      this.stick(side, 0, 0);
      if (e) e.stopPropagation();
    };
    base.addEventListener("pointerdown", (e) => {
      e.preventDefault();
      e.stopPropagation();
      active = e.pointerId;
      try { base.setPointerCapture(e.pointerId); } catch (_) {}
      moveTo(e);
    });
    base.addEventListener("pointermove", (e) => {
      if (active !== e.pointerId) return;
      e.stopPropagation();
      moveTo(e);
    });
    base.addEventListener("pointerup", release);
    base.addEventListener("pointercancel", release);
    base.addEventListener("lostpointercapture", release);
    this[`release${side}`] = () => release(null);
  },


  // ── edit mode ────────────────────────────────────────────────────────────────────────
  //
  // Drag any control where you want it; ‑/+ resize the one last touched. A thumb is not in
  // the same place on a 6" phone as on a tablet, and the default layout is a guess about a
  // hand it has never seen.
  //
  // The whole editor is one capture-phase listener on the root rather than a second set of
  // handlers per control: capture runs before the target's own `pointerdown`, so stopping it
  // there is what keeps a drag from also firing the button.

  el(id) { return this.root && this.root.querySelector(`[data-id="${id}"]`); },

  // The override for a control, created from wherever it currently sits. Seeding it from the
  // live rect is what lets resizing alone work: nothing teleports on the first ‑/+ press.
  slot(id) {
    const L = W.padCfg.layout || (W.padCfg.layout = {});
    if (!L[id]) {
      const el = this.el(id);
      if (!el) return null;
      const r = el.getBoundingClientRect();
      const pr = this.root.getBoundingClientRect();
      L[id] = {
        x: (r.left + r.width / 2 - pr.left) / pr.width,
        y: (r.top + r.height / 2 - pr.top) / pr.height,
        s: 1,
      };
    }
    return L[id];
  },

  // Inline styles, so one moved control does not cost the other fourteen their CSS anchors.
  place(el, o) {
    if (!o) {
      for (const k of ["left", "top", "right", "bottom", "transform"]) el.style[k] = "";
      el.style.removeProperty("--pad-u");
      return;
    }
    el.style.left = `${o.x * 100}%`;
    el.style.top = `${o.y * 100}%`;
    el.style.right = "auto";
    el.style.bottom = "auto";
    el.style.transform = "translate(-50%, -50%)";
    // Every dimension of a control derives from --pad-u, so overriding it on the element is
    // the whole of per-control sizing.
    if (o.s && o.s !== 1) el.style.setProperty("--pad-u", `calc(var(--pad-u0) * ${o.s})`);
    else el.style.removeProperty("--pad-u");
  },

  applyLayout() {
    const L = W.padCfg.layout || {};
    for (const el of this.root.querySelectorAll(".padbtn, .padstick")) {
      this.place(el, L[el.dataset.id]);
    }
  },

  select(el) {
    this.selId = el ? el.dataset.id : "";
    for (const n of this.root.querySelectorAll(".sel")) n.classList.remove("sel");
    if (el) el.classList.add("sel");
  },

  resize(mul) {
    const o = this.slot(this.selId);
    if (!o) return;
    o.s = Math.min(2.5, Math.max(0.5, (o.s || 1) * mul));
    this.place(this.el(this.selId), o);
    this.saveLayout();
  },

  resetOne() {
    if (!this.selId) return;
    delete (W.padCfg.layout || {})[this.selId];
    const el = this.el(this.selId);
    if (el) this.place(el, null);
    this.saveLayout();
  },

  // Back to Rust, which is what persists it — the same round trip every other setting takes.
  saveLayout() {
    emit({ type: "pad_layout", json: JSON.stringify(W.padCfg.layout || {}) });
  },

  buildEditBar() {
    const bar = document.createElement("div");
    bar.className = "padedit";
    for (const [act, glyph, title] of [
      ["small", "－", "Smaller"],
      ["big", "＋", "Bigger"],
      ["reset", "⟲", "Reset this control"],
      ["done", "✓", "Done"],
    ]) {
      const b = document.createElement("button");
      b.dataset.act = act;
      b.title = title;
      b.textContent = glyph;
      b.addEventListener("pointerdown", (e) => {
        e.preventDefault();
        e.stopPropagation();
        if (act === "small") this.resize(1 / 1.12);
        else if (act === "big") this.resize(1.12);
        else if (act === "reset") this.resetOne();
        else emit({ type: "pad_edit", on: false });
      });
      bar.appendChild(b);
    }
    return bar;
  },

  editBind(root) {
    let drag = null;
    root.addEventListener("pointerdown", (e) => {
      if (!this.editing) return;
      const el = e.target.closest(".padbtn, .padstick");
      if (!el) return;
      e.preventDefault();
      e.stopPropagation();
      this.select(el);
      const r = el.getBoundingClientRect();
      const o = this.slot(el.dataset.id);
      // The grab offset, so a control does not jump its centre under the finger.
      drag = {
        el, o,
        pr: root.getBoundingClientRect(),
        ox: r.left + r.width / 2 - e.clientX,
        oy: r.top + r.height / 2 - e.clientY,
      };
      try { el.setPointerCapture(e.pointerId); } catch (_) {}
    }, true);
    root.addEventListener("pointermove", (e) => {
      if (!drag) return;
      e.preventDefault();
      e.stopPropagation();
      const { pr } = drag;
      drag.o.x = Math.min(1, Math.max(0, (e.clientX + drag.ox - pr.left) / pr.width));
      drag.o.y = Math.min(1, Math.max(0, (e.clientY + drag.oy - pr.top) / pr.height));
      this.place(drag.el, drag.o);
    }, true);
    const end = (e) => {
      if (!drag) return;
      drag = null;
      e.stopPropagation();
      this.saveLayout();
    };
    root.addEventListener("pointerup", end, true);
    root.addEventListener("pointercancel", end, true);
  },

  // ── config ───────────────────────────────────────────────────────────────────────────
  apply() {
    const root = this.build();
    if (!root) return;
    const c = W.padCfg;
    root.style.setProperty("--pad-scale", c.scale);
    // Readable while being edited, whatever it is set to for playing.
    root.style.setProperty("--pad-opacity", this.editing ? 0.95 : c.opacity);
    root.style.setProperty("--pad-inset-x", `${c.insetX}px`);
    root.style.setProperty("--pad-inset-y", `${c.insetY}px`);
    root.classList.toggle("editing", !!this.editing);
    if (!this.editing) this.select(null);
    this.applyLayout();
    if (root.hidden !== !c.on) {
      root.hidden = !c.on;
      if (!c.on) this.releaseAll();
    }
  },

  // Everything down goes up. Called when the pad is switched off and when the session ends:
  // a control still held when the overlay disappears is a key nothing can ever release.
  releaseAll() {
    for (const id of [...this.held]) this.press(id, false);
    this.held.clear();
    if (this.releasel) this.releasel();
    if (this.releaser) this.releaser();
    this.look.x = this.look.y = 0;
    this._walk = {};
    this._ax = {};
    if (this.root) {
      for (const el of this.root.querySelectorAll(".padbtn.on")) el.classList.remove("on");
    }
  },
};

// The one entry point Rust calls. Takes the whole config so the panel and the overlay cannot
// drift apart a field at a time.
W.setGamepad = (cfg) => {
  Object.assign(W.padCfg, cfg || {});
  // The layout arrives as text because that is how it is stored: one opaque string Rust
  // never has to know the shape of.
  if (typeof W.padCfg.layout === "string") {
    try { W.padCfg.layout = JSON.parse(W.padCfg.layout || "{}") || {}; }
    catch (_) { W.padCfg.layout = {}; }
  }
  W.gamepad.apply();
};

// Edit mode is session state, not a setting: it is never what you want on a fresh load.
W.setPadEdit = (on) => {
  W.gamepad.editing = !!on;
  // Anything held when the finger starts dragging instead of pressing is held forever.
  W.gamepad.releaseAll();
  W.gamepad.apply();
};
