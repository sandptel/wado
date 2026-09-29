// Runnable check for the tap decision (W.decideTap in js/targets.js) and the pixel heuristic
// (W.lens.edgeFraction in js/lens.js) — the two pure halves of M-P S4/S6.
//
// Run:  node scripts/lens-check.mjs
import { readFileSync } from "node:fs";

const js = (f) => readFileSync(new URL(`../crates/client/src/js/${f}`, import.meta.url), "utf8");
const W = { relayOn() {} };
globalThis.document = { addEventListener() {} };
new Function("W", js("targets.js") + "\n" + js("lens.js"))(W);

let failures = 0;
const run = (name, f) => {
  try { f(); console.log(`ok   ${name}`); }
  catch (e) { failures++; console.log(`FAIL ${name}: ${e.message}`); }
};
const assert = (cond, msg) => { if (!cond) throw new Error(msg); };
// A 400x800 CSS content rect: normalized w 0.1 = 40 css px.
const size = (t) => ({ w: t.w * 400, h: t.h * 800 });
const t = (x, y, w, h) => ({ x, y, w, h, role: "button", name: "" });

run("nothing actionable near: a plain click where the finger was", () => {
  const d = W.decideTap({ x: 0.5, y: 0.5 }, [], size);
  assert(d.kind === "click" && d.at.x === 0.5, JSON.stringify(d));
});

run("one target near: snap onto its centre, even from just outside it", () => {
  const d = W.decideTap({ x: 0.31, y: 0.5 }, [t(0.2, 0.48, 0.1, 0.04)], size);
  assert(d.kind === "snap" && Math.abs(d.at.x - 0.25) < 1e-9, JSON.stringify(d));
});

run("several small ones competing: the lens", () => {
  const bold = t(0.40, 0.1, 0.06, 0.02), ital = t(0.46, 0.1, 0.06, 0.02);
  assert(W.decideTap({ x: 0.46, y: 0.11 }, [bold, ital], size).kind === "lens", "lens");
});

run("finger squarely inside one big target beside others: just click it", () => {
  const big = t(0.1, 0.1, 0.5, 0.2), neighbour = t(0.6, 0.1, 0.05, 0.02);
  const d = W.decideTap({ x: 0.3, y: 0.2 }, [big, neighbour], size);
  assert(d.kind === "click", JSON.stringify(d));
});

run("no tree is not decided here (null → pixel fallback in the caller)", () => {
  assert(W.decideTap({ x: 0, y: 0 }, null, size).kind === "click", "null reads as nothing near");
});

run("flat pixels are calm; a checkerboard of controls is dense", () => {
  const n = 8;
  const flat = new Uint8ClampedArray(n * n * 4).fill(128);
  const busy = new Uint8ClampedArray(n * n * 4);
  for (let i = 0; i < n * n; i++) {
    const v = ((i % n) + Math.floor(i / n)) % 2 ? 255 : 0;
    busy.set([v, v, v, 255], i * 4);
  }
  assert(W.lens.edgeFraction(flat, n) === 0, "flat");
  assert(W.lens.edgeFraction(busy, n) > 0.9, "busy");
});

console.log(failures ? `\n${failures} FAILED` : "\nall passed");
process.exit(failures ? 1 : 0);
