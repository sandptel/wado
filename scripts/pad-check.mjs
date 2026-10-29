// Runnable check for the trackpad's client side: js/pad_link.js, js/pad_prefs.js,
// js/pad_orient.js, js/pad_touch.js, js/pad_keys.js, js/pad_icons.js, js/pad_actions.js,
// js/pad_rail.js (views on the DOM shim in scripts/lib/pad-dom.mjs).
//
// Loads the real modules into a stub `W` with a fake clock, a fake relay and a fake
// RTCPeerConnection, drives them, and asserts the wire messages.
//
// Run:  node scripts/pad-check.mjs
import { readFileSync } from "node:fs";
import { makeDocument } from "./lib/pad-dom.mjs";

const js = (f) => readFileSync(new URL(`../crates/client/src/js/${f}`, import.meta.url), "utf8");
const MODULES = ["pad_link.js", "pad_prefs.js", "pad_orient.js", "pad_touch.js", "pad_keys.js",
  "pad_icons.js", "pad_actions.js", "pad_rail.js"];
const src = MODULES.map(js).join("\n");

class FakeDC {
  constructor(label, opts) { this.label = label; this.opts = opts; this.readyState = "connecting"; this.sent = []; }
  send(s) { this.sent.push(JSON.parse(s)); }
  open() { this.readyState = "open"; if (this.onopen) this.onopen(); }
}

function makeEnv() {
  let now = 0;
  let timers = [];
  const add = (f, ms, every) => { const t = { f, at: now + ms, every }; timers.push(t); return t; };
  const clock = {
    performance: { now: () => now },
    setTimeout: (f, ms) => add(f, ms, 0),
    clearTimeout: (t) => { timers = timers.filter((x) => x !== t); },
    setInterval: (f, ms) => add(f, ms, ms),
    clearInterval: (t) => { timers = timers.filter((x) => x !== t); },
  };
  const relay = [];
  const pcs = [];
  class RTCPeerConnection {
    constructor() { this.dcs = []; this.iceGatheringState = "complete"; this.connectionState = "new"; pcs.push(this); }
    createDataChannel(label, opts) { const dc = new FakeDC(label, opts); this.dcs.push(dc); return dc; }
    async createOffer() { return { type: "offer", sdp: "x" }; }
    async setLocalDescription(d) { this.localDescription = d; }
    async setRemoteDescription(d) { this.remote = d; }
    close() { this.connectionState = "closed"; }
  }
  const docListeners = {};
  const document = makeDocument();
  document.addEventListener = (e, f) => { docListeners[e] = f; };
  const W = { _relayHandlers: {}, clientKey: "dev-1" };
  W.relayOn = (type, fn) => { W._relayHandlers[type] = fn; };
  W.relaySendMsg = (o) => { relay.push(o); return true; };
  const globals = { ...clock, RTCPeerConnection, document, queueMicrotask: (f) => f() };
  new Function("W", ...Object.keys(globals), src)(W, ...Object.values(globals));

  W.configSet = (key, value) => relay.push({ type: "config_set", key, value: String(value ?? ""), confirmed: false });
  const listeners = {};
  const rect = { left: 0, top: 0, width: 1000, height: 1000 };
  const el = {
    getBoundingClientRect: () => ({ ...rect }),
    addEventListener: (e, f) => { listeners[e] = f; },
    removeEventListener: () => {},
    setPointerCapture: () => {},
  };
  const ev = (id, x, y) => ({ pointerId: id, clientX: x, clientY: y, preventDefault: () => {} });
  const advance = (ms) => {
    const end = now + ms;
    for (;;) {
      const due = timers.filter((t) => t.at <= end).sort((a, b) => a.at - b.at)[0];
      if (!due) break;
      now = due.at;
      if (due.every) due.at += due.every; else timers = timers.filter((x) => x !== due);
      due.f();
    }
    now = end;
  };
  const tick = () => new Promise((r) => setImmediate(r));
  return {
    W, relay, pcs, el, rect, advance, tick, docListeners, document,
    out: (msg) => W._relayHandlers.pad_out({ msg }),
    down: (id, x, y) => listeners.pointerdown(ev(id, x, y)),
    move: (id, x, y) => listeners.pointermove(ev(id, x, y)),
    up: (id) => listeners.pointerup(ev(id, 0, 0)),
    cancel: (id) => listeners.pointercancel(ev(id, 0, 0)),
  };
}

let failures = 0;
const run = async (name, steps) => {
  try { await steps(makeEnv()); console.log(`ok   ${name}`); }
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

if (failures) { console.log(`\n${failures} failed`); process.exit(1); }
console.log("\nall pad checks passed");
