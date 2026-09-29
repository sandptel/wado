// Runnable check for the translated-pointer touch gestures in js/input_tap.js.
//
// Loads the real input modules into a stub `W`, drives synthetic pointer events against a
// 1000×1000 video (so a client pixel is 0.001 normalized), and asserts the wire messages.
// Timers and the clock are fake, so a hold is "advance 500 ms", not a sleep.
//
// Run:  node scripts/touch-check.mjs
import { readFileSync } from "node:fs";

const js = (f) => readFileSync(new URL(`../crates/client/src/js/${f}`, import.meta.url), "utf8");
// Bundle order as in bridge.rs, restricted to what the touch path needs.
const src = ["input_core.js", "input_units.js", "input_accel.js", "input_coalesce.js",
  "input_touch.js", "input_tap.js", "input_scroll.js"].map(js).join("\n");

function makeEnv() {
  let now = 0;
  let timers = [];
  let raf = null;
  const clock = {
    performance: { now: () => now },
    setTimeout: (f, ms) => { const t = { f, at: now + ms }; timers.push(t); return t; },
    clearTimeout: (t) => { timers = timers.filter((x) => x !== t); },
    requestAnimationFrame: (f) => { raf = f; return 1; },
    cancelAnimationFrame: () => { raf = null; },
  };
  const sent = [];
  const W = { activePointers: new Set(), gesture: null, moveMode: false, showTouches: false,
    scrollSpeed: 1, naturalScroll: false, touchMode: "pointer", outputScale: 1 };
  new Function("W", ...Object.keys(clock), src)(W, ...Object.values(clock));
  W.sendInput = (o) => sent.push(o);
  const video = {
    getBoundingClientRect: () => ({ left: 0, top: 0, width: 1000, height: 1000 }),
    videoWidth: 1000, videoHeight: 1000,
  };
  const fsm = () => (W.touchMode === "touch" ? W.touchg : W.touchp);
  const flush = () => { if (raf) { const f = raf; raf = null; f(); } };
  const advance = (ms) => {
    now += ms;
    for (const t of timers.filter((x) => x.at <= now)) { clearTimeout(t); t.f(); }
  };
  const ev = (id, x, y) => ({ pointerId: id, clientX: x, clientY: y });
  return {
    W, sent, advance, flush,
    // Dispatched the way input_core.js does, so W.touchMode picks the FSM.
    down: (id, x, y) => fsm().down(ev(id, x, y), video),
    move: (id, x, y) => { fsm().move(ev(id, x, y), video); advance(16); },
    up: (id, x, y) => { fsm().up(ev(id, x, y), video); flush(); },
  };
  function clearTimeout(t) { clock.clearTimeout(t); }
}

let failures = 0;
const run = (name, steps) => {
  try { steps(makeEnv()); console.log(`ok   ${name}`); }
  catch (e) { failures++; console.log(`FAIL ${name}: ${e.message}`); }
};
const assert = (cond, msg) => { if (!cond) throw new Error(msg); };
const buttons = (sent) => sent.filter((m) => m.t === "button");
const near = (a, b) => Math.abs(a - b) < 1e-9;

run("a tap is a left click at the down point", (e) => {
  e.down(1, 100, 200); e.up(1, 102, 201);
  const b = buttons(e.sent);
  assert(b.length === 2, `expected press+release, got ${JSON.stringify(b)}`);
  assert(b[0].button === "left" && b[0].pressed && !b[1].pressed, "left press then release");
  assert(near(b[0].x, 0.1) && near(b[0].y, 0.2), "at the down point");
  assert(!e.sent.some((m) => m.t === "touch"), "no raw wl_touch in pointer mode");
});

run("a quick second tap nearby snaps onto the first", (e) => {
  e.down(1, 100, 200); e.up(1, 100, 200);
  e.advance(150);
  e.down(2, 115, 210); e.up(2, 115, 210);
  const p = buttons(e.sent).filter((m) => m.pressed);
  assert(p.length === 2, "two clicks");
  assert(p[1].x === p[0].x && p[1].y === p[0].y, "second click lands on the first's point");
});

run("a slow or distant second tap is not snapped", (e) => {
  e.down(1, 100, 200); e.up(1, 100, 200);
  e.advance(400);
  e.down(2, 110, 200); e.up(2, 110, 200);
  e.advance(100);
  e.down(3, 300, 300); e.up(3, 300, 300);
  const p = buttons(e.sent).filter((m) => m.pressed);
  assert(near(p[1].x, 0.11), "too slow: own point");
  assert(near(p[2].x, 0.3), "too far: own point");
});

run("hold then lift is a right click, never a left", (e) => {
  e.down(1, 100, 200); e.advance(520); e.up(1, 100, 200);
  const b = buttons(e.sent);
  assert(b.length === 2 && b.every((m) => m.button === "right"), `got ${JSON.stringify(b)}`);
});

run("one-finger drag scrolls with a finger source and stops on lift", (e) => {
  e.down(1, 500, 500);
  for (let y = 490; y >= 400; y -= 10) e.move(1, 500, y);
  e.up(1, 500, 400);
  const s = e.sent.filter((m) => m.t === "scroll");
  assert(s.length >= 2 && s.every((m) => m.source === "finger"), "finger-source axis");
  assert(s.slice(0, -1).every((m) => m.dy > 0), "dragging up scrolls content (positive dy)");
  assert(s.at(-1).stop === true, "terminal axis-stop");
  assert(buttons(e.sent).length === 0, "no click");
});

run("two fingers with the primary moving is a press-and-drag", (e) => {
  e.down(1, 100, 100); e.down(2, 300, 100);
  for (let x = 110; x <= 200; x += 10) e.move(1, x, 100);
  e.move(2, 400, 100); // the second finger moving too must not steer the pointer
  e.up(1, 200, 100);
  const b = buttons(e.sent);
  assert(b.length === 2 && b[0].pressed && !b[1].pressed && b[0].button === "left", "left held");
  assert(near(b[0].x, 0.1), "pressed where the drag started");
  e.flush();
  const m = e.sent.filter((x) => x.t === "pointer_motion");
  assert(m.length && near(m.at(-1).x, 0.2), "pointer followed the primary finger");
  assert(near(b[1].x, 0.2), "released where the primary finger is");
});

run("two fingers spreading is a pinch, not a drag", (e) => {
  e.down(1, 400, 500); e.down(2, 600, 500);
  e.move(2, 700, 500); e.move(1, 300, 500);
  e.up(2, 700, 500);
  const k = e.sent.filter((m) => m.t === "pinch");
  assert(k[0]?.phase === "down" && k.at(-1).phase === "up", "pinch began and ended");
  assert(buttons(e.sent).length === 0, "no button held");
});

run("hold then drag moves the window", (e) => {
  e.down(1, 100, 100); e.advance(520);
  e.move(1, 150, 150); e.up(1, 150, 150);
  const d = e.sent.filter((m) => m.t === "window_drag").map((m) => m.phase);
  assert(d[0] === "down" && d.at(-1) === "up", `window drag, got ${d}`);
  assert(buttons(e.sent).length === 0, "no right click after a move");
});

run("raw mode: two fingers still scroll through the shared helper", (e) => {
  e.W.touchMode = "touch";
  e.down(1, 400, 500); e.down(2, 600, 500);
  for (let y = 490; y >= 400; y -= 10) { e.move(1, 400, y); e.move(2, 600, y); }
  e.up(1, 400, 400);
  const s = e.sent.filter((m) => m.t === "scroll");
  assert(s.length >= 2 && s.slice(0, -1).every((m) => m.dy > 0), "scrolled");
  assert(s.at(-1).stop === true, "terminal axis-stop");
  assert(e.sent.some((m) => m.t === "cancel_touch"), "retracted the first finger's touch");
});

console.log(failures ? `\n${failures} FAILED` : "\nall passed");
process.exit(failures ? 1 : 0);
