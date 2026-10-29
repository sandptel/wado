// Runnable check for the trackpad's client side: js/pad_link.js, js/pad_prefs.js,
// js/pad_orient.js, js/pad_touch.js, js/pad_keys.js, js/pad_icons.js, js/pad_actions.js,
// js/pad_rail.js, js/pad_settings.js (views on the DOM shim in scripts/lib/pad-dom.mjs),
// js/pad_hints.js, js/pad_gestures.js, js/pad_feel.js, js/pad_pointer.js (on the fake clock).
//
// Loads the real modules into a stub `W` with a fake clock, a fake relay and a fake
// RTCPeerConnection, drives them, and asserts the wire messages.
//
// Run:  node scripts/pad-check.mjs
import { readFileSync, writeFileSync, mkdirSync } from "node:fs";
import { makeDocument } from "./lib/pad-dom.mjs";

const js = (f) => readFileSync(new URL(`../crates/client/src/js/${f}`, import.meta.url), "utf8");
const MODULES = ["pad_link.js", "pad_prefs.js", "pad_orient.js", "pad_touch.js", "pad_gestures.js", "pad_keys.js",
  "pad_feel.js", "pad_icons.js", "pad_actions.js", "pad_rail.js", "pad_hints.js", "pad_pointer.js", "pad_settings.js"];
const src = MODULES.map(js).join("\n");

class FakeDC {
  constructor(label, opts, record) { this.label = label; this.opts = opts; this.readyState = "connecting"; this.sent = []; this.record = record; }
  send(s) { this.sent.push(JSON.parse(s)); this.record(this.label, JSON.parse(s)); }
  open() { this.readyState = "open"; if (this.onopen) this.onopen(); }
}

// A browser's localStorage; pass one store to two envs to stand in for a reload.
function makeStorage() {
  const m = new Map();
  return { m, getItem: (k) => (m.has(k) ? m.get(k) : null), setItem: (k, v) => m.set(k, String(v)), removeItem: (k) => m.delete(k) };
}

// `faithful`: the pad's area is a DOM-shim element with every pointer module attached the way
// js/pad_ui.js attaches them, and microtasks run as a browser runs them — after each listener
// and each timer returns — instead of at once. The other checks keep the simpler stand-in.
function makeEnv({ storage = makeStorage(), hostConfig = null, faithful = false } = {}) {
  let now = 0;
  let timers = [];
  const add = (f, ms, every) => { const t = { f, at: now + ms, every }; timers.push(t); return t; };
  const micro = [];
  const drain = () => { while (micro.length) micro.shift()(); };
  // Animation frames: a vsync every 16 ms on the fake clock runs what was requested before it.
  let frames = [];
  let frameCount = 0;
  const paintErrors = [];
  const clock = {
    performance: { now: () => now },
    setTimeout: (f, ms) => add(f, ms, 0),
    clearTimeout: (t) => { timers = timers.filter((x) => x !== t); },
    setInterval: (f, ms) => add(f, ms, ms),
    clearInterval: (t) => { timers = timers.filter((x) => x !== t); },
    requestAnimationFrame: (f) => { const t = { f }; frames.push(t); return t; },
    cancelAnimationFrame: (t) => { frames = frames.filter((x) => x !== t); },
  };
  add(() => {
    frameCount++;
    const due = frames;
    frames = [];
    // The shim does not parse innerHTML, so a painter that looks inside its own markup throws
    // here. That is the shim's gap, and painting never sends, so it is kept apart from checks.
    for (const t of due) { try { t.f(now); } catch (err) { paintErrors.push(err.message); } drain(); }
  }, 16, 16);
  const relay = [];
  const log = []; // every message out, by any path, in order: [ms, path, msg]
  const record = (path, msg) => log.push([now, path, msg]);
  const pcs = [];
  class RTCPeerConnection {
    constructor() { this.dcs = []; this.iceGatheringState = "complete"; this.connectionState = "new"; pcs.push(this); }
    createDataChannel(label, opts) { const dc = new FakeDC(label, opts, record); this.dcs.push(dc); return dc; }
    async createOffer() { return { type: "offer", sdp: "x" }; }
    async setLocalDescription(d) { this.localDescription = d; }
    async setRemoteDescription(d) { this.remote = d; }
    close() { this.connectionState = "closed"; }
  }
  const docListeners = {};
  const document = makeDocument();
  document.addEventListener = (e, f) => { docListeners[e] = f; };
  const W = { _relayHandlers: {}, clientKey: "dev-1", hostConfig };
  W.relayOn = (type, fn) => { W._relayHandlers[type] = fn; };
  W.relaySendMsg = (o) => { relay.push(o); record("relay", o); return true; };
  const globals = { ...clock, RTCPeerConnection, document, localStorage: storage,
    queueMicrotask: faithful ? (f) => micro.push(f) : (f) => f(),
    addEventListener: () => {}, removeEventListener: () => {} };
  new Function("W", ...Object.keys(globals), src)(W, ...Object.values(globals));

  W.configSet = (key, value) => relay.push({ type: "config_set", key, value: String(value ?? ""), confirmed: false });
  const listeners = {};
  const rect = { left: 0, top: 0, width: 1000, height: 1000 };
  const reads = { n: 0 }; // getBoundingClientRect calls on the pad
  const el = faithful ? document.createElement("div") : {};
  Object.assign(el, {
    getBoundingClientRect: () => { reads.n++; return { ...rect }; },
    addEventListener: (e, f) => { (listeners[e] ||= []).push(f); },
    removeEventListener: (e, f) => { listeners[e] = (listeners[e] || []).filter((x) => x !== f); },
    setPointerCapture: () => {},
  });
  W.padPointer.attach(el); // the pad's only pointer listeners, as js/pad_ui.js attaches them
  if (faithful) for (const m of ["padTouch", "padGestures", "padFeel", "padHints"]) W[m].attach(el); // js/pad_ui.js's order
  const ev = (id, x, y) => ({ pointerId: id, clientX: x, clientY: y, preventDefault: () => {} });
  // A browser runs each listener, then the microtasks it queued, before the next listener.
  const fire = (type, e) => { for (const f of listeners[type] || []) { f(e); drain(); } };
  const advance = (ms) => {
    const end = now + ms;
    for (;;) {
      const due = timers.filter((t) => t.at <= end).sort((a, b) => a.at - b.at)[0];
      if (!due) break;
      now = due.at;
      if (due.every) due.at += due.every; else timers = timers.filter((x) => x !== due);
      due.f();
      drain();
    }
    now = end;
  };
  const tick = () => new Promise((r) => setImmediate(r)).then(drain);
  return {
    W, relay, log, pcs, el, rect, reads, advance, tick, docListeners, document, now: () => now,
    frames: () => frameCount, paintErrors,
    out: (msg) => W._relayHandlers.pad_out({ msg }),
    down: (id, x, y) => fire("pointerdown", ev(id, x, y)),
    move: (id, x, y) => fire("pointermove", ev(id, x, y)),
    up: (id) => fire("pointerup", ev(id, 0, 0)),
    cancel: (id) => fire("pointercancel", ev(id, 0, 0)),
  };
}

let failures = 0;
const run = async (name, steps, opts) => {
  try { await steps(makeEnv(opts)); console.log(`ok   ${name}`); }
  catch (e) { failures++; console.log(`FAIL ${name}: ${e.message}`); }
};
const assert = (cond, msg) => { if (!cond) throw new Error(msg); };
const eq = (a, b, msg) => assert(JSON.stringify(a) === JSON.stringify(b), `${msg}: ${JSON.stringify(a)} ≠ ${JSON.stringify(b)}`);
const padIn = (relay, k) => relay.filter((m) => m.type === "pad_in" && (!k || m.msg.k === k)).map((m) => m.msg);

// Bring the fake direct link up: answer the offer, open both channels.
async function goDirect(e) {
  await e.tick();
  e.W._relayHandlers.pad_answer({ sdp: JSON.stringify({ type: "answer", sdp: "y" }), err: "" });
  await e.tick();
  for (const dc of e.pcs[0].dcs) dc.open();
  await e.tick();
}

// ── link ──────────────────────────────────────────────────────────────────────────────
await run("relayed at once: a frame rides the relay as pad_in", async (e) => {
  e.W.pad.start();
  assert(e.W.pad.state === "relayed", "relayed straight away");
  e.W.pad.send({ k: "frame", seq: 1, t: 0, c: [] }, true);
  eq(padIn(e.relay, "frame")[0], { k: "frame", seq: 1, t: 0, c: [] }, "relayed frame");
});

await run("both channels open → direct, touch channel unordered, size re-sent first", async (e) => {
  e.W.padTouch.attach(e.el);
  e.W.pad.start();
  await goDirect(e);
  assert(e.W.pad.state === "direct", `state ${e.W.pad.state}`);
  const [touch, keys] = e.pcs[0].dcs;
  eq([touch.label, touch.opts], ["pad-touch", { ordered: false, maxRetransmits: 0 }], "touch channel");
  eq(keys.label, "pad-keys", "keys channel");
  eq(keys.sent[0], { k: "size", w: 1000, h: 1000 }, "size re-sent on the keys channel");
  assert(touch.sent[0].k === "frame", "frame re-sent on the touch channel");
  e.W.pad.send({ k: "frame", seq: 9, t: 0, c: [] }, true);
  assert(touch.sent.at(-1).seq === 9, "lossy now goes direct");
});

await run("a refused offer stops the pad and says why", async (e) => {
  const got = [];
  e.W.pad.onMsg((m) => got.push(m));
  e.W.pad.start();
  await e.tick();
  e.W._relayHandlers.pad_answer({ sdp: "", err: "this device may not use the trackpad — on the computer: wado allow phone input" });
  await e.tick();
  assert(e.W.pad.state === "off", `state ${e.W.pad.state}`);
  assert(got.some((m) => m.k === "err" && m.message.includes("wado allow phone input")), "err delivered");
});

await run("pong sets rtt", async (e) => {
  e.W.pad.start();
  e.advance(30);
  e.W._relayHandlers.pad_out({ msg: { k: "pong", t: 0 } });
  eq(e.W.pad.rtt, 30, "rtt");
});

// ── touch ─────────────────────────────────────────────────────────────────────────────
await run("direct: down, move, up → three frames with normalised coordinates", async (e) => {
  e.W.padTouch.attach(e.el);
  e.W.pad.start();
  await goDirect(e);
  const touch = e.pcs[0].dcs[0];
  touch.sent.length = 0;
  e.down(1, 500, 500); e.move(1, 510, 500); e.up(1);
  eq(touch.sent.map((m) => m.c), [[[0, 32768, 32768]], [[0, 33423, 32768]], []], "frames");
});

await run("relayed: ten moves within 16 ms → one frame after the down", async (e) => {
  e.W.padTouch.attach(e.el);
  e.W.pad.start();
  e.down(1, 100, 100);
  const before = padIn(e.relay, "frame").length;
  for (let i = 0; i < 10; i++) e.move(1, 100 + i, 100);
  e.advance(16);
  eq(padIn(e.relay, "frame").length - before, 1, "coalesced");
});

await run("a quick tap is never coalesced away on the relay", async (e) => {
  e.W.padTouch.attach(e.el);
  e.W.pad.start();
  e.down(1, 100, 100); e.up(1);
  eq(padIn(e.relay, "frame").map((m) => m.c.length), [1, 0], "down then lift");
});

await run("a resting finger is re-sent as keepalive", async (e) => {
  e.W.padTouch.attach(e.el);
  e.W.pad.start();
  e.down(1, 100, 100);
  const before = padIn(e.relay, "frame").length;
  e.advance(250);
  const after = padIn(e.relay, "frame").slice(before);
  assert(after.length >= 2, `keepalives: ${after.length}`);
  assert(after.every((m) => JSON.stringify(m.c) === JSON.stringify([[0, 6554, 6554]])), "same contacts");
});

await run("ids: lowest free, reused after a lift; cancel lifts", async (e) => {
  e.W.padTouch.attach(e.el);
  e.W.pad.start();
  e.down(1, 0, 0); e.down(2, 0, 0);
  eq(e.W.padTouch.frame().map((c) => c[0]), [0, 1], "two ids");
  e.up(1); e.down(3, 0, 0);
  eq(e.W.padTouch.frame().map((c) => c[0]), [0, 1], "id 0 reused");
  e.cancel(2); e.cancel(3);
  eq(padIn(e.relay, "frame").at(-1).c, [], "cancel lifts");
});

// ── landscape (js/pad_orient.js) ──────────────────────────────────────────────────────
// Turned 90°, the phone is held with its top to the left: "right as held" is down the screen.
await run("toPad at 90°: moving right as held is +x, down as held is +y", async (e) => {
  const O = e.W.padOrient;
  O.rotate(90);
  const r = { left: 0, top: 0, width: 400, height: 800 };
  const a = O.toPad(200, 100, r), b = O.toPad(200, 300, r), c = O.toPad(100, 100, r);
  assert(b.x > a.x && b.y === a.y, `right as held: ${JSON.stringify([a, b])}`);
  assert(c.y > a.y && c.x === a.x, `down as held (screen left): ${JSON.stringify([a, c])}`);
  eq([a.w, a.h], [800, 400], "the pad's own size is landscape");
});

// Turned −90°, the top is to the right: "right as held" is up the screen.
await run("toPad at −90°: moving right as held is +x, down as held is +y", async (e) => {
  const O = e.W.padOrient;
  O.rotate(-90);
  const r = { left: 0, top: 0, width: 400, height: 800 };
  const a = O.toPad(200, 500, r), b = O.toPad(200, 300, r), c = O.toPad(300, 500, r);
  assert(b.x > a.x && b.y === a.y, `right as held: ${JSON.stringify([a, b])}`);
  assert(c.y > a.y && c.x === a.x, `down as held (screen right): ${JSON.stringify([a, c])}`);
});

await run("turned: Size is landscape and a frame is in the pad's frame", async (e) => {
  Object.assign(e.rect, { width: 400, height: 800 }); // the turned pad's upright bounding box
  e.W.padOrient.rotate(90);
  e.W.pad.start();
  e.W.padTouch.attach(e.el);
  eq(padIn(e.relay, "size").at(-1), { k: "size", w: 800, h: 400 }, "landscape size");
  e.down(1, 200, 0); // top of the screen = the left edge of the pad as held
  eq(padIn(e.relay, "frame").at(-1).c, [[0, 0, 32768]], "left edge, middle");
  e.move(1, 200, 400); e.advance(16);
  eq(padIn(e.relay, "frame").at(-1).c, [[0, 32768, 32768]], "moved right as held");
});

await run("turning mid-touch lifts first, then sends the new size", async (e) => {
  e.W.padTouch.attach(e.el);
  e.W.pad.start();
  e.down(1, 100, 100);
  const at = e.relay.length;
  Object.assign(e.rect, { width: 1000, height: 600 });
  e.W.padOrient.rotate(90);
  const after = padIn(e.relay.slice(at)).filter((m) => m.k !== "ping");
  eq(after.map((m) => m.k), ["frame", "size"], "lift, then size");
  eq([after[0].c, after[1]], [[], { k: "size", w: 600, h: 1000 }], "empty frame, turned size");
  e.move(1, 200, 200); e.advance(200);
  assert(padIn(e.relay, "frame").slice(-1)[0].c.length === 0, "the old finger stays lifted");
});

await run("a flip keeps the size but still lifts the fingers", async (e) => {
  Object.assign(e.rect, { width: 400, height: 800 });
  e.W.padOrient.rotate(90);
  e.W.padTouch.attach(e.el);
  e.W.pad.start();
  e.down(1, 100, 100);
  const sizes = padIn(e.relay, "size").length;
  e.W.padOrient.rotate(-90);
  eq(padIn(e.relay, "frame").at(-1).c, [], "lifted");
  eq(padIn(e.relay, "size").length, sizes, "same size, not re-sent");
});

// ── rail and quick actions (js/pad_actions.js, js/pad_rail.js) ───────────────────────────
const ACTIONS = [
  { id: "workspace-next", name: "Next workspace", group: "Workspace", icon: "right" },
  { id: "fullscreen", name: "Fullscreen", group: "Window", icon: "max" },
  { id: "play-pause", name: "Play / pause", group: "Media", icon: "play" },
];
function railEnv(e, { rail = "actions", quick, landscape = true } = {}) {
  const box = { rail: e.document.createElement("div"), bar: e.document.createElement("div") };
  e.W.padRail.mount(box);
  e.W.pad.start();
  e.W.padPrefs.set("rail", rail);
  if (quick) e.W.padPrefs.set("quick", quick);
  e.W.padOrient.rotate(landscape ? 90 : 0);
  if (!landscape) e.W.padOrient.layout = "portrait";
  return box;
}
const actionIds = (n) => n.querySelectorAll("[data-action]").map((b) => b.dataset.action);

await run("the rail shows `quick` in order and skips ids the computer did not list", async (e) => {
  const box = railEnv(e, { quick: ["fullscreen", "no-such-thing", "workspace-next"] });
  e.W.padActions.refresh();
  eq(padIn(e.relay, "actions"), [{ k: "actions" }], "asked for the list");
  e.out({ k: "action_list", actions: ACTIONS });
  e.W.padRail.paint();
  eq(actionIds(box.rail), ["fullscreen", "workspace-next"], "rail order");
  assert(box.bar.hidden, "no portrait bar in landscape");
  eq(e.W.padRail.items(["play-pause", "play-pause", "x"], ACTIONS).map((a) => a.id), ["play-pause"], "deduped");
});

await run("a tap on a quick action sends {k:run, id}", async (e) => {
  const box = railEnv(e, { quick: ["play-pause"] });
  e.out({ k: "action_list", actions: ACTIONS });
  e.W.padRail.paint();
  box.rail.querySelector('[data-action="play-pause"]').click();
  eq(padIn(e.relay, "run"), [{ k: "run", id: "play-pause" }], "run sent");
  assert(box.rail.querySelector('[data-action="play-pause"]').dataset.state === "run", "spinner while it runs");
});

await run("a Ran { ok: false } shows its message and marks the button", async (e) => {
  const box = railEnv(e, { quick: ["fullscreen"] });
  const errs = [];
  e.W.padRail.onError((m) => errs.push(m));
  e.out({ k: "action_list", actions: ACTIONS });
  e.W.padRail.paint();
  const b = box.rail.querySelector('[data-action="fullscreen"]');
  b.click();
  e.out({ k: "ran", id: "fullscreen", ok: false, message: "no window to make fullscreen" });
  await e.tick(); e.advance(200); await e.tick();
  eq(errs, ["no window to make fullscreen"], "the words reach the alert card");
  eq(b.dataset.state, "bad", "shake state");
  e.advance(400);
  eq(b.dataset.state, undefined, "and back to rest");
});

await run("a Ran { ok: true } ticks inside 400 ms", async (e) => {
  const box = railEnv(e, { quick: ["fullscreen"] });
  e.out({ k: "action_list", actions: ACTIONS });
  e.W.padRail.paint();
  const b = box.rail.querySelector('[data-action="fullscreen"]');
  b.click();
  e.out({ k: "ran", id: "fullscreen", ok: true, message: "" });
  await e.tick(); e.advance(110); await e.tick();
  eq(b.dataset.state, "ok", "check");
  e.advance(280);
  eq(b.dataset.state, undefined, "done by 390 ms");
});

await run("no host: the refusal is shown in the rail, not as an alert, and the pad keeps working", async (e) => {
  const box = railEnv(e, {});
  const seen = [];
  e.W.pad.onMsg((m) => { if (m.k === "err") seen.push(e.W.padActions.owns(m)); });
  e.out({ k: "action_list", actions: [] });
  e.out({ k: "err", message: "this device may not run actions — on the computer: wado allow 'Pixel' host" });
  e.W.padRail.paint();
  eq(seen, [true], "the err is the list's");
  assert(box.rail.textContent.includes("wado allow 'Pixel' host"), "copy in the rail");
  eq(e.W.pad.state, "relayed", "pad still up");
});

await run("portrait with rail = actions: a quick bar above the pad", async (e) => {
  const box = railEnv(e, { quick: ["play-pause", "fullscreen"], landscape: false });
  e.out({ k: "action_list", actions: ACTIONS });
  e.W.padRail.paint();
  eq(actionIds(box.bar), ["play-pause", "fullscreen"], "bar order");
  assert(box.rail.hidden, "no rail in portrait");
});

await run("rail = keys: modifiers, Esc, Tab, arrows and Type", async (e) => {
  const box = railEnv(e, { rail: "keys" });
  e.W.padRail.paint();
  eq(box.rail.querySelectorAll("[data-mod]").map((b) => b.dataset.mod), ["super", "ctrl", "alt", "shift"], "mods");
  eq(box.rail.querySelectorAll("[data-key]").map((b) => b.dataset.key), ["esc", "tab", "left", "right", "up", "down"], "keys");
  box.rail.querySelector('[data-key="esc"]').click();
  eq(padIn(e.relay, "key"), [{ k: "key", key: "esc", mods: [] }], "Esc sent");
  assert(box.rail.querySelector(".tptype"), "Type button");
});

await run("the icon set matches ui/widgets/icon.rs", async (e) => {
  const rs = readFileSync(new URL("../crates/client/src/ui/widgets/icon.rs", import.meta.url), "utf8");
  const names = [...rs.matchAll(/^\s+"([a-z0-9]+)" => "/gm)].map((m) => m[1]);
  eq(e.W.padIconNames, names, "same names, same order");
});

// ── settings (js/pad_prefs.js, js/pad_settings.js) ──────────────────────────────────────
const configSets = (relay) => relay.filter((m) => m.type === "config_set").map((m) => [m.key, m.value]);
function sheetEnv(e, actions = ACTIONS) {
  const root = e.document.createElement("div");
  e.W.pad.start();
  e.W.padSettings.mount(root);
  if (actions) e.out({ k: "action_list", actions });
  e.W.padSettings.open();
  const sheet = root.querySelector(".tpsheet");
  return { root, sheet, click: (sel) => { sheet.querySelector(sel).click(); e.advance(200); } };
}

await run("settings: choosing the rail sends ConfigSet device.<own key>.trackpad.rail", async (e) => {
  const s = sheetEnv(e);
  s.click('[data-v="none"]');
  eq(configSets(e.relay), [["device.dev-1.trackpad.rail", "none"]], "config_set");
  eq(e.W.padPrefs.get().rail, "none", "applied");
  assert(s.sheet.querySelector('[data-v="none"]').getAttribute("aria-checked") === "true", "re-rendered on");
});

await run("settings: switches save landscape, flip, fullscreen and keyboard as true/false", async (e) => {
  const s = sheetEnv(e);
  s.click('[data-key="sw:tps-Keyboardopenswiththepad"]');
  s.click('[data-key="sw:tps-Flip"]');
  s.click('[data-key="sw:tps-Landscape"]');
  eq(configSets(e.relay), [["device.dev-1.trackpad.keyboard", "true"], ["device.dev-1.trackpad.flip", "true"],
    ["device.dev-1.trackpad.landscape", "false"]], "config_sets");
  assert(s.sheet.querySelector('[data-key="sw:tps-Flip"]').disabled, "flip greys out with landscape off");
});

await run("settings: quick actions switch on at the end, ↑ moves up, saved as a JSON list", async (e) => {
  const s = sheetEnv(e);
  e.W.padPrefs.set("quick", ["fullscreen"]);
  e.relay.length = 0;
  s.click('[data-key="act:play-pause"]');
  s.click('[data-key="mv:play-pause:-1"]');
  eq(configSets(e.relay), [["device.dev-1.trackpad.quick", '["fullscreen","play-pause"]'],
    ["device.dev-1.trackpad.quick", '["play-pause","fullscreen"]']], "quick saves");
  eq(s.sheet.querySelector(".tps-chosen").querySelectorAll("[data-id]").map((r) => r.dataset.id), ["play-pause", "fullscreen"], "chosen order");
  s.click('[data-key="act:fullscreen"]');
  eq(configSets(e.relay).at(-1), ["device.dev-1.trackpad.quick", '["play-pause"]'], "switched off");
});

await run("settings: a gesture picks an action or Off", async (e) => {
  const s = sheetEnv(e);
  const sel = s.sheet.querySelector("#tpsg-edge-left");
  sel.value = "workspace-next"; sel.dispatch("change");
  eq(configSets(e.relay).at(-1), ["device.dev-1.trackpad.gestures.edge-left", "workspace-next"], "assigned");
  eq(e.W.padPrefs.get().gestures, { "edge-left": "workspace-next" }, "in prefs");
  const again = s.sheet.querySelector("#tpsg-edge-left");
  eq(again.value, "workspace-next", "re-rendered with it");
  again.value = ""; again.dispatch("change");
  eq(configSets(e.relay).at(-1), ["device.dev-1.trackpad.gestures.edge-left", ""], "Off clears it");
});

await run("settings: without host, one line with the grant copy and no pickers", async (e) => {
  const s = sheetEnv(e, null);
  e.out({ k: "action_list", actions: [] });
  e.out({ k: "err", message: "this device may not run actions — on the computer: wado allow 'Pixel' host" });
  assert(s.sheet.textContent.includes("wado allow 'Pixel' host"), "grant copy");
  eq(s.sheet.querySelectorAll("select").length, 0, "no gesture pickers");
});

{
  const storage = makeStorage();
  await run("a reload reads the settings back from the mirror", async (e) => {
    e.W.padPrefs.set("rail", "actions");
  }, { storage });
  await run("…in the reloaded page", async (e) => {
    eq(e.W.padPrefs.get().rail, "actions", "mirror");
  }, { storage });
}

await run("another browser on the same device key: the daemon's copy wins", async (e) => {
  eq(e.W.padPrefs.get().rail, "keys", "defaults before the daemon answers");
  const seen = [];
  e.W.padPrefs.onChange((p) => seen.push(p.rail));
  e.W._relayHandlers.config_state({ state: { trackpad: { landscape: true, fullscreen: false, flip: true, rail: "none",
    quick: ["lock"], gestures: { "four-tap": "lock" }, keyboard: false } } });
  const p = e.W.padPrefs.get();
  eq([p.rail, p.flip, p.quick, p.gestures], ["none", true, ["lock"], { "four-tap": "lock" }], "adopted");
  eq(seen, ["none"], "told the layout");
});

await run("a config_state already there at load is read too", async (e) => {
  eq(e.W.padPrefs.get().rail, "actions", "from W.hostConfig");
}, { hostConfig: { trackpad: { rail: "actions" } } });

// ── phone gestures (js/pad_gestures.js) ─────────────────────────────────────────────────
const GACTIONS = [...ACTIONS, { id: "lock", name: "Lock screen", group: "System", icon: "lock" },
  { id: "workspace-prev", name: "Previous workspace", group: "Workspace", icon: "left" }];

await run("a gesture whose action this computer does not offer is not claimed", async (e) => {
  e.W.pad.start();
  e.out({ k: "action_list", actions: ACTIONS }); // no workspace-prev here
  e.W.padPrefs.setGesture("edge-left", "workspace-prev");
  e.W.padTouch.attach(e.el);
  e.W.padGestures.attach(e.el);
  e.relay.length = 0;
  e.down(1, 5, 500);
  eq(padIn(e.relay, "frame").map((m) => m.c.length), [1], "forwarded at once");
});
function gEnv(e, gestures = {}, attach = true) {
  e.W.pad.start();
  e.out({ k: "action_list", actions: GACTIONS });
  for (const [g, id] of Object.entries(gestures)) e.W.padPrefs.setGesture(g, id);
  if (attach) { e.W.padTouch.attach(e.el); e.W.padGestures.attach(e.el); }
  e.relay.length = 0;
}
const frames = (relay) => padIn(relay, "frame");
const runs = (relay) => padIn(relay, "run").map((m) => m.id);
const ALL = { "edge-left": "workspace-prev", "edge-right": "workspace-next", "four-tap": "lock", "two-hold": "play-pause" };

await run("edge-left: unassigned, a touch at the edge goes straight through", async (e) => {
  gEnv(e, {});
  e.down(1, 5, 500);
  eq(frames(e.relay).map((m) => m.c.length), [1], "forwarded at once");
});

await run("edge-left: assigned, a swipe in is claimed, never forwarded, and runs its action", async (e) => {
  gEnv(e, { "edge-left": "workspace-prev" });
  e.down(1, 8, 500); e.advance(20); e.move(1, 30, 502); e.advance(20); e.move(1, 60, 505); e.advance(100);
  e.up(1); e.advance(100);
  assert(frames(e.relay).every((m) => m.c.length === 0), `no contact forwarded: ${JSON.stringify(frames(e.relay))}`);
  eq(runs(e.relay), ["workspace-prev"], "ran");
});

await run("edge-right is the right edge as held, also when the pad is turned", async (e) => {
  Object.assign(e.rect, { width: 400, height: 800 }); // turned 90°: the pad's right edge is the screen's bottom
  e.W.padOrient.rotate(90);
  gEnv(e, { "edge-right": "workspace-next" });
  e.down(1, 200, 795); e.advance(20); e.move(1, 200, 740); e.advance(40);
  eq(runs(e.relay), ["workspace-next"], "ran");
  e.up(1);
});

await run("an edge touch that becomes a drag is forwarded, ≤ 60 ms late on its first frame only", async (e) => {
  gEnv(e, { "edge-left": "workspace-prev" });
  e.down(1, 5, 500); e.advance(20); e.move(1, 7, 540); e.advance(30);
  eq(frames(e.relay).length, 0, "held");
  e.advance(10); // 60 ms: decided
  const first = frames(e.relay)[0];
  assert(first && first.c.length === 1 && first.t <= 60, `first frame at ${first && first.t}`);
  eq(first.c[0][1], 328, "where it landed (x = 5 of 1000)");
  e.move(1, 9, 600); e.advance(16);
  const last = frames(e.relay).at(-1);
  assert(last.t <= 60 + 16 + 16 && last.c[0][2] === 39321, `the next move goes as usual: ${JSON.stringify(last)}`);
  eq(runs(e.relay), [], "no action");
});

await run("a tap at the edge is forwarded as a tap", async (e) => {
  gEnv(e, { "edge-left": "workspace-prev" });
  e.down(1, 5, 500); e.advance(30); e.up(1);
  eq(frames(e.relay).map((m) => m.c.length), [1, 0], "down then lift");
  eq(runs(e.relay), [], "no action");
});

await run("four-tap: unassigned, the four-finger touch is forwarded and nothing runs", async (e) => {
  gEnv(e, {});
  for (let i = 1; i <= 4; i++) { e.down(i, 200 + i * 80, 500); e.advance(10); }
  for (let i = 1; i <= 4; i++) { e.up(i); e.advance(10); }
  assert(frames(e.relay).some((m) => m.c.length === 4), "four contacts reached the laptop");
  eq(runs(e.relay), [], "nothing claimed");
});

await run("four-tap: assigned, claimed — no four-contact frame, lifted past the tap timeout, runs", async (e) => {
  gEnv(e, { "four-tap": "lock" });
  for (let i = 1; i <= 4; i++) { e.down(i, 200 + i * 80, 500); e.advance(10); }
  for (let i = 1; i <= 4; i++) { e.up(i); e.advance(10); }
  e.advance(300);
  assert(!frames(e.relay).some((m) => m.c.length === 4), "the fourth finger never went");
  const lift = frames(e.relay).find((m) => m.c.length === 0);
  assert(lift && lift.t >= 200, `lifted at ${lift && lift.t}`);
  eq(runs(e.relay), ["lock"], "ran");
});

await run("four-tap assigned, but a four-finger swipe: handed back to the laptop", async (e) => {
  gEnv(e, { "four-tap": "lock" });
  for (let i = 1; i <= 4; i++) { e.down(i, 200 + i * 80, 500); e.advance(5); }
  for (let i = 1; i <= 4; i++) e.move(i, 200 + i * 80 + 30, 500);
  e.advance(16);
  assert(frames(e.relay).some((m) => m.c.length === 4), "all four on the laptop once it moved");
  for (let i = 1; i <= 4; i++) e.up(i);
  e.advance(300);
  eq(runs(e.relay), [], "not a tap");
});

await run("two-hold: assigned, two still fingers are lifted on the laptop, then the action runs", async (e) => {
  gEnv(e, { "two-hold": "play-pause" });
  e.down(1, 400, 500); e.down(2, 500, 500);
  e.advance(650);
  const msgs = padIn(e.relay).filter((m) => m.k === "frame" || m.k === "run");
  const at = msgs.findIndex((m) => m.k === "run");
  assert(at > 0 && msgs[at - 1].k === "frame" && msgs[at - 1].c.length === 0, "empty frame first, then run");
  eq(runs(e.relay), ["play-pause"], "ran");
  e.move(1, 450, 520); e.advance(200); e.up(1); e.up(2);
  assert(frames(e.relay).slice(-1)[0].c.length === 0, "claimed fingers stay off the laptop");
});

await run("two-hold: a two-finger scroll is never claimed — it moved", async (e) => {
  gEnv(e, { "two-hold": "play-pause" });
  e.down(1, 400, 500); e.down(2, 500, 500);
  e.advance(100); e.move(1, 400, 530); e.move(2, 500, 530);
  e.advance(800);
  eq(runs(e.relay), [], "no claim");
  assert(frames(e.relay).at(-1).c.length === 2, "both fingers still on the laptop");
});

await run("a three-finger swipe is forwarded exactly as without gestures, with every gesture assigned", async () => {
  const drive = (e) => {
    for (let i = 1; i <= 3; i++) { e.down(i, 300 + i * 60, 500); e.advance(8); }
    for (let s = 1; s <= 6; s++) { for (let i = 1; i <= 3; i++) e.move(i, 300 + i * 60 + s * 20, 500); e.advance(16); }
    for (let i = 1; i <= 3; i++) e.up(i);
    e.advance(50);
    return frames(e.relay);
  };
  const a = makeEnv(); gEnv(a, ALL);
  const b = makeEnv(); gEnv(b, {}); b.W.padGestures.detach();
  eq(drive(a), drive(b), "identical frames");
  eq(runs(a.relay), [], "nothing ran");
});

// ── gesture hints (js/pad_hints.js): what the picture says the fingers are doing ───────
const F = (x0, y0, x, y) => ({ x0, y0, x, y });
await run("hints: two fingers together scroll; spreading pinches; three slide a swipe; four are four", async (e) => {
  const c = e.W.padHints.classify;
  const sc = c([F(100, 100, 100, 160), F(150, 100, 150, 160)], 100);
  eq([sc.kind, sc.axis], ["scroll", "v"], "two fingers down the pad");
  const pi = c([F(100, 100, 60, 100), F(150, 100, 190, 100)], 100);
  eq([pi.kind, pi.out, pi.scale.toFixed(1)], ["pinch", true, "2.6"], "spread");
  eq(c([F(100, 100, 120, 100), F(150, 100, 130, 100)], 100).kind, "pinch", "close");
  const sw = c([F(0, 0, 40, 0), F(50, 0, 90, 0), F(100, 0, 140, 0)], 100);
  eq([sw.kind, sw.n, sw.axis], ["swipe", 3, "h"], "three to the right");
  eq(c([F(0, 0, 2, 0), F(50, 0, 50, 0), F(100, 0, 100, 0), F(150, 0, 150, 0)], 50).kind, "four", "four still");
  eq(c([F(0, 0, 3, 0)], 200), null, "one finger: not yet a hold");
  const h = c([F(0, 0, 3, 0)], 900);
  eq([h.kind, h.progress], ["hold", 1], "held still and full");
  eq(c([F(0, 0, 30, 0)], 900), null, "a moving finger is no hold");
  eq(c([F(0, 0, 5, 5), F(50, 0, 55, 5)], 100), null, "too little to tell");
});

// ── keys ──────────────────────────────────────────────────────────────────────────────
await run("diffText", async (e) => {
  const d = e.W.padKeys.diffText;
  eq(d("hello", "hello w"), { back: 0, ins: " w" }, "append");
  eq(d("teh", "the "), { back: 2, ins: "he " }, "autocorrect");
  eq(d("a😀", "a"), { back: 1, ins: "" }, "one code point");
});

function field() {
  const l = {};
  return { value: "", addEventListener: (e, f) => { l[e] = f; }, removeEventListener: () => {}, l };
}
// K.attach seeds the field with the sentinel; every direct f.value assignment below stands in
// for the real keyboard's full field content, so it must keep the sentinel on the front.
const SENTINEL = "​";

await run("latched Super then the keyboard's 1 → a shortcut, not text", async (e) => {
  e.W.pad.start();
  const f = field();
  e.W.padKeys.attach(f);
  e.W.padKeys.tapMod("super");
  f.value = SENTINEL + "1"; f.l.input({ isComposing: false });
  eq(padIn(e.relay).filter((m) => m.k !== "ping"), [{ k: "char", ch: "1", mods: ["super"] }], "char");
  eq([f.value, e.W.padKeys.mods.super], [SENTINEL, "off"], "field cleared, latch off");
});

await run("double-tap locks a modifier (hold), tap again releases", async (e) => {
  e.W.pad.start();
  e.W.padKeys.tapMod("ctrl");
  e.advance(100);
  e.W.padKeys.tapMod("ctrl");
  eq(e.W.padKeys.mods.ctrl, "locked", "locked");
  e.advance(500);
  e.W.padKeys.tapMod("ctrl");
  eq(padIn(e.relay, "hold"), [{ k: "hold", mods: ["ctrl"] }, { k: "hold", mods: [] }], "holds");
});

await run("dictation commits one text; autocorrect is backspaces + text", async (e) => {
  e.W.pad.start();
  const f = field();
  e.W.padKeys.attach(f);
  f.l.compositionstart();
  f.value = SENTINEL + "Hello"; f.l.input({ isComposing: true });
  f.value = SENTINEL + "Hello there."; f.l.compositionend();
  eq(padIn(e.relay, "text"), [{ k: "text", text: "Hello there." }], "one text");
  f.value = SENTINEL + "Hello there. teh"; f.l.input({ isComposing: false });
  f.value = SENTINEL + "Hello there. the "; f.l.input({ isComposing: false });
  const tail = padIn(e.relay).filter((m) => m.k !== "ping").slice(2);
  eq(tail, [{ k: "key", key: "backspace", mods: [] }, { k: "key", key: "backspace", mods: [] }, { k: "text", text: "he " }], "correction");
});

await run("Enter: flushes the text, then presses enter", async (e) => {
  e.W.pad.start();
  const f = field();
  e.W.padKeys.attach(f);
  f.value = SENTINEL + "ls"; f.l.input({ isComposing: false });
  f.l.keydown({ key: "Enter", isComposing: false, preventDefault: () => {} });
  eq(padIn(e.relay).filter((m) => m.k !== "ping"), [{ k: "text", text: "ls" }, { k: "key", key: "enter", mods: [] }], "order");
});

await run("Gboard's Unidentified backspace on an empty field: sentinel gone → one backspace, restored", async (e) => {
  e.W.pad.start();
  const f = field();
  e.W.padKeys.attach(f);
  eq(f.value, SENTINEL, "field seeded with the sentinel");
  f.value = ""; f.l.input({ isComposing: false }); // Gboard deleted it; no keydown at all
  eq(padIn(e.relay).filter((m) => m.k !== "ping"), [{ k: "key", key: "backspace", mods: [] }], "one backspace");
  eq(f.value, SENTINEL, "sentinel restored");
});

// ── lifecycle (from the branch review) ─────────────────────────────────────────────────
await run("stop says bye and settles a pending offer, so a reopen can go direct", async (e) => {
  e.W.pad.start();
  await e.tick();
  e.W.pad.stop();
  eq(padIn(e.relay, "bye"), [{ k: "bye" }], "bye through the relay");
  await e.tick();
  const offers = () => e.relay.filter((m) => m.type === "pad_offer").length;
  const before = offers();
  e.W.pad.start();
  await e.tick();
  eq(offers(), before + 1, "a fresh offer after reopening");
});

await run("a fatal err from the daemon stops the pad", async (e) => {
  e.W.pad.start();
  e.W._relayHandlers.pad_out({ msg: { k: "err", message: "The trackpad needs /dev/uinput", fatal: true } });
  eq(e.W.pad.state, "off", "stopped");
});

await run("direct: a lift is repeated at +20 and +60 ms", async (e) => {
  e.W.padTouch.attach(e.el);
  e.W.pad.start();
  await goDirect(e);
  const touch = e.pcs[0].dcs[0];
  e.down(1, 500, 500);
  e.advance(100);
  touch.sent.length = 0;
  e.up(1);
  e.advance(70);
  eq(touch.sent.map((m) => m.c), [[], [], []], "lift sent three times");
});

await run("locked Ctrl then the keyboard's c → a shortcut", async (e) => {
  e.W.pad.start();
  const f = field();
  e.W.padKeys.attach(f);
  e.W.padKeys.tapMod("ctrl"); e.advance(50); e.W.padKeys.tapMod("ctrl");
  f.value = SENTINEL + "c"; f.l.input({ isComposing: false });
  eq(padIn(e.relay, "char"), [{ k: "char", ch: "c", mods: [] }], "char with Ctrl already held");
});

await run("switching to direct re-holds locked modifiers", async (e) => {
  e.W.padTouch.attach(e.el);
  const f = field();
  e.W.padKeys.attach(f);
  e.W.pad.start();
  e.W.padKeys.tapMod("super"); e.advance(50); e.W.padKeys.tapMod("super");
  await goDirect(e);
  const keys = e.pcs[0].dcs[1];
  assert(keys.sent.some((m) => m.k === "hold" && m.mods[0] === "super"), "hold re-sent on the keys channel");
});

// ── one pointer pipeline (js/pad_pointer.js) ─────────────────────────────────────────────
// Every message out, by every path, timed, for the touch and gesture scenarios above, driven
// with the whole pad attached (touch, gestures, rings, hints) and browser-order microtasks.
// `scripts/fixtures/pad-sends.json` was recorded from the pad before js/pad_pointer.js, when
// each module listened on its own (commit c86b6d8): the send path must match it byte for byte.
// Re-record only for a deliberate wire change:  node scripts/pad-check.mjs --record
const SENDS = new URL("./fixtures/pad-sends.json", import.meta.url);
const SCENARIOS = {
  "direct down, move, up": async (e) => {
    e.W.pad.start(); await goDirect(e);
    e.down(1, 500, 500); e.move(1, 510, 500); e.up(1); e.advance(100);
  },
  "direct two fingers, several moves per frame": async (e) => {
    e.W.pad.start(); await goDirect(e);
    e.down(1, 300, 500); e.advance(4); e.down(2, 500, 500);
    for (let s = 1; s <= 8; s++) { for (let k = 0; k < 2; k++) { e.move(1, 300 + s * 9 + k, 500); e.move(2, 500 + s * 9 + k, 500); } e.advance(16); }
    e.up(1); e.advance(3); e.up(2); e.advance(100);
  },
  "relayed coalescing, tap, keepalive, cancel": async (e) => {
    e.W.pad.start();
    e.down(1, 100, 100); for (let i = 0; i < 10; i++) e.move(1, 100 + i, 100); e.advance(16);
    e.advance(250); e.up(1); e.advance(16);
    e.down(2, 0, 0); e.down(3, 0, 0); e.up(2); e.down(4, 0, 0); e.cancel(3); e.cancel(4); e.advance(50);
  },
  "turned, then turning and flipping mid-touch": async (e) => {
    Object.assign(e.rect, { width: 400, height: 800 });
    e.W.padOrient.rotate(90); e.W.pad.start();
    e.down(1, 200, 0); e.move(1, 200, 400); e.advance(16);
    Object.assign(e.rect, { width: 1000, height: 600 }); e.W.padOrient.rotate(-90);
    e.move(1, 200, 200); e.advance(200); e.up(1);
    e.down(2, 300, 300); e.advance(16); e.W.padOrient.rotate(90); e.advance(50);
  },
  "gestures: edge swipe, edge drag, edge tap": async (e) => {
    gEnv(e, { "edge-left": "workspace-prev" }, false);
    e.down(1, 8, 500); e.advance(20); e.move(1, 30, 502); e.advance(20); e.move(1, 60, 505); e.advance(100); e.up(1); e.advance(100);
    e.down(2, 5, 500); e.advance(20); e.move(2, 7, 540); e.advance(40); e.move(2, 9, 600); e.advance(16); e.up(2); e.advance(50);
    e.down(3, 5, 500); e.advance(30); e.up(3); e.advance(100);
  },
  "gestures: edge-right turned": async (e) => {
    Object.assign(e.rect, { width: 400, height: 800 }); e.W.padOrient.rotate(90);
    gEnv(e, { "edge-right": "workspace-next" }, false);
    e.down(1, 200, 795); e.advance(20); e.move(1, 200, 740); e.advance(40); e.up(1); e.advance(50);
  },
  "gestures: four-tap claimed, then a four-finger swipe handed back": async (e) => {
    gEnv(e, { "four-tap": "lock" }, false);
    for (let i = 1; i <= 4; i++) { e.down(i, 200 + i * 80, 500); e.advance(10); }
    for (let i = 1; i <= 4; i++) { e.up(i); e.advance(10); }
    e.advance(300);
    for (let i = 1; i <= 4; i++) { e.down(i, 200 + i * 80, 500); e.advance(5); }
    for (let i = 1; i <= 4; i++) e.move(i, 200 + i * 80 + 30, 500);
    e.advance(16); for (let i = 1; i <= 4; i++) e.up(i); e.advance(300);
  },
  "gestures: two-hold claimed, a two-finger scroll not": async (e) => {
    gEnv(e, { "two-hold": "play-pause" }, false);
    e.down(1, 400, 500); e.down(2, 500, 500); e.advance(650);
    e.move(1, 450, 520); e.advance(200); e.up(1); e.up(2); e.advance(50);
    e.down(3, 400, 500); e.down(4, 500, 500); e.advance(100); e.move(3, 400, 530); e.move(4, 500, 530); e.advance(800);
    e.up(3); e.up(4); e.advance(50);
  },
  "gestures: a three-finger swipe with every gesture assigned": async (e) => {
    gEnv(e, ALL, false);
    for (let i = 1; i <= 3; i++) { e.down(i, 300 + i * 60, 500); e.advance(8); }
    for (let s = 1; s <= 6; s++) { for (let i = 1; i <= 3; i++) e.move(i, 300 + i * 60 + s * 20, 500); e.advance(16); }
    for (let i = 1; i <= 3; i++) e.up(i);
    e.advance(50);
  },
};
{
  const got = {};
  for (const [name, steps] of Object.entries(SCENARIOS)) {
    const e = makeEnv({ faithful: true });
    await steps(e);
    await e.tick(); // the offer settles
    got[name] = JSON.parse(JSON.stringify(e.log));
  }
  if (process.argv.includes("--record")) {
    mkdirSync(new URL("./fixtures/", import.meta.url), { recursive: true });
    writeFileSync(SENDS, JSON.stringify(got, null, 1) + "\n");
    console.log(`recorded ${SENDS.pathname}`);
  }
  const want = JSON.parse(readFileSync(SENDS, "utf8"));
  for (const name of Object.keys(SCENARIOS)) {
    await run(`same sends as before the pipeline: ${name}`, async () => {
      assert(got[name].length > 0, "nothing was sent");
      eq(got[name], want[name], "sends");
    });
  }
}

await run("the pad's box is read at most once per animation frame, however many pointers move", async () => {
  const e = makeEnv({ faithful: true });
  e.W.pad.start();
  for (let i = 1; i <= 3; i++) e.down(i, 200 + i * 100, 500);
  e.advance(16);
  const before = { reads: e.reads.n, frames: e.frames() };
  let moves = 0;
  for (let f = 1; f <= 20; f++) {
    for (let k = 0; k < 2; k++) for (let i = 1; i <= 3; i++) { e.move(i, 200 + i * 100 + f * 5 + k, 500); moves++; }
    e.advance(16);
  }
  const reads = e.reads.n - before.reads, frames = e.frames() - before.frames;
  console.log(`     getBoundingClientRect: ${reads} reads for ${moves} moves over ${frames} frames`);
  assert(reads <= frames, `${reads} reads in ${frames} frames`);
});

await run("the pictures get each pointer after its frame is sent, and one failing breaks nothing", async () => {
  const e = makeEnv({ faithful: true });
  e.W.pad.start();
  const order = [];
  const send = e.W.pad.send;
  e.W.pad.send = (m, lossy) => { if (m.k === "frame") order.push("send"); return send(m, lossy); };
  const feel = e.W.padFeel.input;
  e.W.padFeel.input = (t, p) => { order.push("feel"); feel(t, p); throw new Error("a broken picture"); };
  const hints = e.W.padHints.input;
  e.W.padHints.input = (t, p) => { order.push("hints"); return hints(t, p); };
  const quiet = console.error;
  console.error = () => {};
  try { e.down(1, 500, 500); } finally { console.error = quiet; }
  eq(order, ["send", "feel", "hints"], "frame first, then the rings, then the hints");
  eq(e.el.querySelectorAll(".tpfinger").length, 1, "a ring was drawn");
  eq(frames(e.relay).at(-1).c, [[0, 32768, 32768]], "the frame went");
});

if (failures) { console.log(`\n${failures} failed`); process.exit(1); }
console.log("\nall pad checks passed");
