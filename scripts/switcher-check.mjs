// Runnable check for the switcher dial's pure physics and layout (W.dialMath in
// js/switcher.js). The DOM half is judged by a human on a phone; this pins what the numbers do.
//
// Run:  node scripts/switcher-check.mjs
import { readFileSync } from "node:fs";

const src = readFileSync(new URL("../crates/client/src/js/switcher.js", import.meta.url), "utf8");
const W = {};
globalThis.document = { addEventListener() {} }; // the dial registers one listener at load
new Function("W", src)(W);
const M = W.dialMath;

let failures = 0;
const run = (name, f) => {
  try { f(); console.log(`ok   ${name}`); }
  catch (e) { failures++; console.log(`FAIL ${name}: ${e.message}`); }
};
const assert = (cond, msg) => { if (!cond) throw new Error(msg); };

run("the spring overshoots once and settles on its target", () => {
  let p = 0, v = 0, peak = 0, t = 0;
  while (t < 2000) {
    [p, v] = M.step(p, v, 1, 4);
    peak = Math.max(peak, p);
    t += 4;
    if (Math.abs(p - 1) < 0.002 && Math.abs(v) < 0.0005) break;
  }
  assert(peak > 1.03, `expected a visible bounce past the target, peak ${peak.toFixed(3)}`);
  assert(peak < 1.35, `bounce too wild, peak ${peak.toFixed(3)}`);
  assert(t < 1200, `took ${t} ms to settle`);
});

run("a slow release rounds to the nearest window", () => {
  assert(M.settle(1.4, 0, 5) === 1 && M.settle(1.6, 0, 5) === 2, "rounding");
});

run("a flick carries past the nearest window, but never off the ends", () => {
  assert(M.settle(1.2, 0.02, 9) === 4, `flick forward, got ${M.settle(1.2, 0.02, 9)}`);
  assert(M.settle(1, 1, 3) === 2 && M.settle(1, -1, 3) === 0, "clamped");
});

run("dragging past either end gives, but only a little", () => {
  assert(M.rubber(-1, 3) === -0.35 && M.rubber(3, 3) === 2.35, "rubber band");
  assert(M.rubber(1.5, 3) === 1.5, "no effect inside");
});

run("the centre is biggest; odd counts show exactly their slots", () => {
  const c = M.item(0, 1), n = M.item(1, 1), out = M.item(2, 1);
  assert(c.scale === 1 && n.scale < 1, "neighbours smaller");
  assert(c.opacity === 1 && n.opacity > 0.3, "3-up: neighbours visible");
  assert(out.opacity === 0, "3-up: the next one out is hidden");
  assert(M.item(2, 2).opacity > 0, "5-up: two either side visible");
  assert(M.item(-1, 1).offset === -M.SPACING, "symmetric");
});

console.log(failures ? `\n${failures} FAILED` : "\nall passed");
process.exit(failures ? 1 : 0);
