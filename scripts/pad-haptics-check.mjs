// The web touchpad's haptic model (crates/client/src/js/pad_haptics.js), fed by hand in Node:
// the same cases as the app's test/pad_haptics_test.dart. Exits non-zero on a failure.
//   node scripts/pad-haptics-check.mjs
import { readFileSync } from "node:fs";
import assert from "node:assert/strict";

const src = readFileSync(new URL("../crates/client/src/js/pad_haptics.js", import.meta.url), "utf8");
function model() {
  const W = { padTouch: { input: () => {} } };
  new Function("W", src)(W);
  const felt = [];
  W.padHaptics.record((f) => felt.push(f));
  W.padTouch.input("down", { id: 0, x: 0, y: 0, w: 400, h: 300 });
  W.padTouch.input("up", { id: 0, x: 0, y: 0 });
  felt.length = 0;
  return { h: W.padHaptics, felt };
}
const moveAll = (h, ids, at) => ids.forEach((id) => h.move(id, ...at(id)));

{ // tap clicks; a quick second touch that moves grabs; lifting drops
  const { h, felt } = model();
  h.down(1, 100, 100, 10000); h.up(1, 10080);
  h.down(1, 100, 100, 10150); h.move(1, 110, 100); h.move(1, 130, 100); h.up(1, 10400);
  assert.deepEqual(felt, ["click", "grab", "drop"]);
}
{ // two fingers lifting one at a time: a right click
  const { h, felt } = model();
  h.down(1, 100, 100, 10000); h.down(2, 140, 100, 10005); h.up(1, 10060); h.up(2, 10070);
  assert.deepEqual(felt, ["right"]);
}
{ // three fingers up: a swell, a detent past half way, an undo coming back
  const { h, felt } = model();
  [1, 2, 3].forEach((id) => h.down(id, 100 + 40 * id, 250, 10000));
  for (let k = 1; k <= 10; k++) moveAll(h, [1, 2, 3], (id) => [100 + 40 * id, 250 - 10 * k]);
  assert.deepEqual(felt, ["swipe", "detent"]);
  for (let k = 1; k <= 6; k++) moveAll(h, [1, 2, 3], (id) => [100 + 40 * id, 150 + 10 * k]);
  assert.deepEqual(felt, ["swipe", "detent", "undo"]);
}
{ // four fingers spreading: a pinch, then its detent
  const { h, felt } = model();
  const at = (id, r) => [[200 - r, 150], [200 + r, 150], [200, 150 - r], [200, 150 + r]][id - 1];
  [1, 2, 3, 4].forEach((id) => h.down(id, ...at(id, 30), 10000));
  for (let k = 1; k <= 9; k++) moveAll(h, [1, 2, 3, 4], (id) => at(id, 30 + 8 * k));
  assert.equal(felt[0], "pinch");
  assert.ok(felt.includes("detent"), felt.join());
}
console.log("pad haptics: ok");
