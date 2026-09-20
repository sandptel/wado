// Does the on-screen pad's edit mode actually work? Two things it has already got wrong:
//
//   1. the D-pad drifted apart under a drag — four buttons instead of one cross;
//   2. the layout did not survive a reload, back when it travelled through Rust.
//
// Both are pure DOM logic, so they are answerable in Node against a shim rather than on a
// phone. `js/gamepad.js` is loaded as-is: the point is to test the shipped file, not a copy.
//
// ponytail: one file, no framework, no fixtures. `node scripts/padlayout-check.mjs`.
import { readFileSync } from "node:fs";
import assert from "node:assert";

const SRC = new URL("../crates/client/src/js/gamepad.js", import.meta.url);

// ── the smallest DOM that gamepad.js will run against ─────────────────────────
const RECTS = new Map(); // el -> [x,y,w,h]
class El {
  constructor(tag) {
    this.tag = tag; this.children = []; this.parent = null;
    this.dataset = {}; this.listeners = {}; this._cls = new Set();
    this.style = {
      _p: {},
      setProperty: (k, v) => { this.style._p[k] = v; },
      removeProperty: (k) => { delete this.style._p[k]; },
    };
  }
  set className(v) { this._cls = new Set(String(v).split(/\s+/).filter(Boolean)); }
  get className() { return [...this._cls].join(" "); }
  get classList() {
    const c = this._cls;
    return {
      add: (x) => c.add(x), remove: (x) => c.delete(x), contains: (x) => c.has(x),
      toggle: (x, on) => (on === undefined ? (c.has(x) ? c.delete(x) : c.add(x)) : on ? c.add(x) : c.delete(x)),
    };
  }
  appendChild(k) { k.parent = this; this.children.push(k); return k; }
  addEventListener(t, fn, cap) { (this.listeners[t] ||= []).push([fn, !!cap]); }
  setPointerCapture() {}
  getBoundingClientRect() {
    const [x, y, w, h] = RECTS.get(this) || [0, 0, 40, 40];
    return { left: x, top: y, width: w, height: h };
  }
  closest(sel) {
    const want = sel.split(",").map((s) => s.trim().replace(/^\./, ""));
    for (let n = this; n; n = n.parent) if (want.some((w) => n._cls.has(w))) return n;
    return null;
  }
  *walk() { yield this; for (const k of this.children) yield* k.walk(); }
  querySelector(sel) { return this.querySelectorAll(sel)[0] || null; }
  querySelectorAll(sel) {
    const parts = sel.split(",").map((s) => s.trim());
    const out = [];
    for (const n of this.walk()) {
      if (n === this) continue;
      for (const p of parts) {
        const attr = p.match(/^\[data-id="(.*)"\]$/);
        if (attr) { if (n.dataset.id === attr[1]) out.push(n); continue; }
        if (p.split(".").filter(Boolean).every((c) => n._cls.has(c))) out.push(n);
      }
    }
    return out;
  }
}
const chain = (el) => { const c = []; for (let n = el; n; n = n.parent) c.unshift(n); return c; };
function dispatch(el, type, ev) {
  ev = { type, target: el, preventDefault() {}, stopPropagation() {}, ...ev };
  const path = chain(el);
  for (const n of path) for (const [fn, cap] of n.listeners[type] || []) if (cap) fn(ev);
  for (const n of [...path].reverse()) for (const [fn, cap] of n.listeners[type] || []) if (!cap) fn(ev);
}

// One store shared by every instance, because that is what a reload is: a new page against the
// same browser storage.
const store = new Map();
const localStorage = {
  getItem: (k) => (store.has(k) ? store.get(k) : null),
  setItem: (k, v) => store.set(k, String(v)),
  removeItem: (k) => store.delete(k),
};

// A fresh "page". The stage is 1000x500; #wado-pad needs its own entry or every drag clamps to
// one pixel — a harness artefact that reads exactly like a clamping bug.
function fresh() {
  const mount = new El("div");
  RECTS.set(mount, [0, 0, 1000, 500]);
  const document = {
    createElement: (t) => new El(t),
    getElementById: (id) => (id === "wado-pad-mount" ? mount : null),
  };
  const W = { coalesce: { now() {}, queue() {}, add() {} } };
  const src = readFileSync(SRC, "utf8");
  new Function("W", "emit", "document", "window", "requestAnimationFrame", "localStorage", src)(
    W, () => {}, document, { devicePixelRatio: 1 }, () => 0, localStorage,
  );
  W.setGamepad({ on: true, mode: "pad", scale: 1, opacity: 0.5, insetX: 0, insetY: 0 });
  const root = mount.children[0];
  RECTS.set(root, [0, 0, 1000, 500]); // #wado-pad is inset:0 inside the mount
  // Nothing lays anything out here, so every control would otherwise share one rect and the
  // cluster would have no shape to preserve. Give the cross its arms.
  const at = (id, x, y) => RECTS.set(root.querySelector(`[data-id="${id}"]`), [x, y, 40, 40]);
  at("up", 100, 300); at("left", 60, 340); at("right", 140, 340); at("down", 100, 380);
  return { W, root };
}
const slotOf = (W, id) => (W.padCfg.layout || {})[id];

// ── 1. the D-pad moves as one piece ───────────────────────────────────────────
const a = fresh();
a.W.setPadEdit(true);
const up = a.root.querySelector('[data-id="up"]');
const before = ["up", "left", "right", "down"].map((id) => {
  const r = RECTS.get(a.root.querySelector(`[data-id="${id}"]`));
  return { id, x: (r[0] + 20) / 1000, y: (r[1] + 20) / 500 };
});
dispatch(up, "pointerdown", { clientX: 120, clientY: 320, pointerId: 1 });
dispatch(up, "pointermove", { clientX: 320, clientY: 370, pointerId: 1 });
dispatch(up, "pointerup", { clientX: 320, clientY: 370, pointerId: 1 });

const deltas = before.map(({ id, x, y }) => {
  const o = slotOf(a.W, id);
  assert(o, `${id} was not moved by the drag — the cluster did not travel as one`);
  return { id, dx: o.x - x, dy: o.y - y };
});
const d0 = deltas[0];
for (const d of deltas) {
  assert(Math.abs(d.dx - d0.dx) < 1e-9 && Math.abs(d.dy - d0.dy) < 1e-9,
    `${d.id} moved ${d.dx},${d.dy} but ${d0.id} moved ${d0.dx},${d0.dy} — the cross came apart`);
}
assert(Math.abs(d0.dx - 0.2) < 1e-9 && Math.abs(d0.dy - 0.1) < 1e-9,
  `the cluster travelled ${d0.dx},${d0.dy}, expected 0.2,0.1`);

// ── 2. resize spreads the arms rather than overlapping them ───────────────────
const spread = (W) => {
  const xs = ["up", "left", "right", "down"].map((id) => slotOf(W, id).x);
  return Math.max(...xs) - Math.min(...xs);
};
const s0 = spread(a.W);
a.W.gamepad.resize(1.5);
assert(spread(a.W) > s0 * 1.4, `resize kept the arms ${spread(a.W)} apart, was ${s0}`);
assert(slotOf(a.W, "up").s > 1, "resize did not scale the control itself");

// ── 3. a reload picks the layout back up ──────────────────────────────────────
const want = { ...slotOf(a.W, "up") };
const b = fresh();
const got = slotOf(b.W, "up");
assert(got, "a fresh instance found no stored layout — it did not survive the reload");
assert(Math.abs(got.x - want.x) < 1e-9 && Math.abs(got.y - want.y) < 1e-9,
  `reload placed up at ${got.x},${got.y}, stored was ${want.x},${want.y}`);
assert(b.root.querySelector('[data-id="up"]').style.left === `${want.x * 100}%`,
  "the stored layout was read but never applied to the element");

// ── 4. Reset the layout clears it, in storage too ─────────────────────────────
b.W.resetPadLayout();
assert(!slotOf(b.W, "up"), "reset left the layout in memory");
assert(!slotOf(fresh().W, "up"), "reset left the layout in storage");

console.log("OK — cluster drag, cluster resize, reload, reset");
