// Does the in-session gamepad's layout hold? The overlay (`js/gamepad.js`) now draws the session's
// layout from the one model (`js/gpad/`), and it is arranged in the full-screen editor. The four
// things this check has always asked, on the new model:
//
//   1. a cluster moves as one piece — the face buttons are a group, the d-pad one control, and a
//      drag moves the whole group by the same amount;
//   2. a resize spreads a group around its centre rather than overlapping its buttons;
//   3. the layout survives a reload (the localStorage mirror, before the daemon answers);
//   4. Reset the layout puts the built-in back, in storage too.
//
// And what the overlay is for: it draws the session's layout, presses go out on the session's own
// input channel in the device's mode ("pad": gamepad_button; "keys": key events), and the console's
// Gamepad face packs the same layout into its deck slot.
//
//   dx build -p wado-client --platform web
//   node scripts/padlayout-check.mjs        (needs Chrome; run with the sandbox off)

import { join } from "node:path";
import { existsSync } from "node:fs";
import assert from "node:assert/strict";
import { openPage } from "./lib/cdp-page.mjs";

const ROOT = new URL("..", import.meta.url).pathname;
const PUB = process.env.WADO_PUB || join(ROOT, "target/dx/wado-client/debug/web/public");
if (!existsSync(join(PUB, "index.html"))) { console.error("no built client at " + PUB); process.exit(1); }
const P = await openPage({ pub: PUB, http: 8821, cdp: 9421 });
let failed = 0;
const check = async (name, fn) => {
  try { await fn(); console.log("  ok  " + name); } catch (e) { failed++; console.log("FAIL  " + name + "\n        " + String(e.message || e).split("\n").join("\n        ")); }
};

const session = async (fresh) => {
  await P.size(844, 390);
  await P.load("?fixture");
  if (fresh) await P.js(`localStorage.clear(); true`);
  await P.load("?fixture");
  await P.fx({ type: "sessionOn" }, { type: "link", state: "direct", path: "lan", rtt_ms: 23, net: "wifi" });
  await P.sleep(700);
  // The session's wire, recorded; the overlay on, in the cockpit.
  await P.js(`(() => { const W = window.__wado; W.__co = []; W.coalesce.now = (m) => W.__co.push(m); W.coalesce.queue = (k, m) => W.__co.push(m);
    document.documentElement.dataset.arr = "full"; W.setGamepad({ on: true }); return true; })()`);
  await P.sleep(400);
};
const rect = (sel, id) => P.js(`(() => { const e = document.querySelector(${JSON.stringify(sel)})._gp.get(${JSON.stringify(id)}); const r = e.getBoundingClientRect(); return [r.left, r.top, r.width, r.height]; })()`);

try {
  await session(true);
  await check("the overlay draws the session's layout over the picture", async () => {
    const ids = await P.js(`[...document.querySelector("#wado-pad")._gp.keys()].sort()`);
    assert.deepEqual(ids, ["a", "b", "back", "dpad", "guide", "lb", "lt", "rb", "rt", "start", "stick-l", "stick-r", "x", "y"]);
  });
  await check("a press goes out on the session's channel: pad mode is gamepad_button, keys mode a key", async () => {
    await P.js(`window.__wado.gpadPrefs.set("drives", "pad"); true`);
    await P.sleep(200);
    let [x, y, w, h] = await rect("#wado-pad", "a");
    await P.tap(x + w / 2, y + h / 2);
    await P.sleep(100);
    let co = await P.js("window.__wado.__co");
    assert.deepEqual(co.slice(-2), [{ t: "gamepad_button", code: 0x130, pressed: true }, { t: "gamepad_button", code: 0x130, pressed: false }]);
    await P.js(`window.__wado.gpadPrefs.set("drives", "keys"); true`);
    await P.sleep(200);
    [x, y, w, h] = await rect("#wado-pad", "a");
    await P.tap(x + w / 2, y + h / 2);
    await P.sleep(100);
    co = await P.js("window.__wado.__co");
    assert.deepEqual(co.slice(-2), [{ t: "key", code: 57, pressed: true }, { t: "key", code: 57, pressed: false }]);
  });
  await check("1. a cluster moves as one piece: the face group travels together in the editor", async () => {
    await P.js(`window.__wado.setPadEdit(true); true`);
    await P.sleep(700);
    const before = {};
    for (const id of ["a", "b", "x", "y"]) before[id] = await rect("#wado-gpad .gppa", id);
    const [x, y, w, h] = before.a;
    await P.drag([[x + w / 2, y + h / 2], [x + w / 2 - 20, y + h / 2], [x + w / 2 - 61, y + h / 2 - 21]]);
    await P.sleep(500);
    const d = {};
    for (const id of ["a", "b", "x", "y"]) { const r = await rect("#wado-gpad .gppa", id); d[id] = [r[0] - before[id][0], r[1] - before[id][1]]; }
    for (const id of ["b", "x", "y"]) assert.ok(Math.abs(d[id][0] - d.a[0]) < 0.5 && Math.abs(d[id][1] - d.a[1]) < 0.5, `${id} moved ${d[id]}, a ${d.a} — the cluster came apart`);
    assert.ok(Math.abs(d.a[0]) > 40, `it moved: ${d.a}`);
  });
  await check("2. a resize spreads the group around its centre — buttons grow and never overlap", async () => {
    const spread = async () => { const a = await rect("#wado-gpad .gppa", "x"), b = await rect("#wado-gpad .gppa", "b"); return b[0] - a[0]; };
    const s0 = await spread();
    const before = await rect("#wado-gpad .gppa", "a");
    await P.js(`window.__wado.gpadEdit.setScale(1.5); true`);
    await P.sleep(300);
    const after = await rect("#wado-gpad .gppa", "a");
    assert.ok(after[2] > before[2] * 1.3, `A grew ${before[2]} → ${after[2]}`);
    assert.ok(await spread() > s0 * 1.0, "the group spread with it");
    const [x, b] = [await rect("#wado-gpad .gppa", "x"), await rect("#wado-gpad .gppa", "a")];
    assert.ok(x[0] + x[2] <= b[0] + 1 || b[1] >= x[1] + x[3] - 1, "X and A do not overlap");
    await P.js(`document.querySelector("#wado-gpad .gpdone").click(); true`);
    await P.sleep(400);
  });
  const saved = await P.js(`JSON.stringify(window.__wado.gpadPrefs.layoutFor("session").controls.find((c) => c.id === "a").l)`);
  await check("3. a reload picks the layout back up, before the daemon has said anything", async () => {
    await session(false);
    const now = await P.js(`JSON.stringify(window.__wado.gpadPrefs.layoutFor("session").controls.find((c) => c.id === "a").l)`);
    assert.equal(now, saved);
    assert.notEqual(now, JSON.stringify([0.86, 0.78, 1]), "it is the edited one");
  });
  await check("4. Reset the layout puts the built-in back, in storage too", async () => {
    await P.js(`window.__wado.resetPadLayout(); true`);
    await P.sleep(300);
    assert.equal(await P.js(`window.__wado.gpadPrefs.get().layout`), "xbox");
    await session(false);
    assert.equal(await P.js(`window.__wado.gpadPrefs.get().layout`), "xbox");
  });
  await check("the console's Gamepad face packs the layout into its deck slot, every control inside it and ≥ 44 px", async () => {
    await P.js(`(() => { const W = window.__wado; W.consolePrefs.set("face", "gamepad"); document.documentElement.dataset.arr = "console"; return true; })()`);
    await P.sleep(900);
    const r = await P.js(`(() => { const pad = document.querySelector("#wado-pad"); const b = pad.getBoundingClientRect();
      return { box: [b.left, b.top, b.width, b.height], ctl: [...pad._gp.values()].map((e) => { const q = e.getBoundingClientRect(); return [e.dataset.id, q.left - b.left, q.top - b.top, q.width, q.height]; }) }; })()`);
    assert.ok(r.box[2] > 0 && r.box[3] > 0, "the deck slot has a size: " + r.box);
    for (const [id, x, y, w, h] of r.ctl) {
      assert.ok(Math.min(w, h) >= 43.5, `${id} ${w}×${h}`);
      assert.ok(x > -1 && y > -1 && x + w < r.box[2] + 1 && y + h < r.box[3] + 1, `${id} outside the deck: ${[x, y, w, h]} in ${r.box}`);
    }
  });
} finally {
  P.close();
}
console.log(failed ? `${failed} failed` : "OK — overlay, wire, cluster drag, cluster resize, reload, reset, console deck");
process.exit(failed ? 1 : 0);
