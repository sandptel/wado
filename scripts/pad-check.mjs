// Runnable check for the trackpad's client side: js/pad_link.js, js/pad_touch.js, js/pad_keys.js.
//
// Loads the real modules into a stub `W` with a fake clock, a fake relay and a fake
// RTCPeerConnection, drives them, and asserts the wire messages.
//
// Run:  node scripts/pad-check.mjs
import { readFileSync } from "node:fs";

const js = (f) => readFileSync(new URL(`../crates/client/src/js/${f}`, import.meta.url), "utf8");
const src = ["pad_link.js", "pad_touch.js", "pad_keys.js"].map(js).join("\n");

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
  const document = { visibilityState: "visible", addEventListener: (e, f) => { docListeners[e] = f; }, removeEventListener: () => {} };
  const W = { _relayHandlers: {} };
  W.relayOn = (type, fn) => { W._relayHandlers[type] = fn; };
  W.relaySendMsg = (o) => { relay.push(o); return true; };
  const globals = { ...clock, RTCPeerConnection, document, queueMicrotask: (f) => f() };
  new Function("W", ...Object.keys(globals), src)(W, ...Object.values(globals));

  const listeners = {};
  const el = {
    getBoundingClientRect: () => ({ left: 0, top: 0, width: 1000, height: 1000 }),
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
    W, relay, pcs, el, advance, tick, docListeners,
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

await run("latched Super then the keyboard's 1 → a shortcut, not text", async (e) => {
  e.W.pad.start();
  const f = field();
  e.W.padKeys.attach(f);
  e.W.padKeys.tapMod("super");
  f.value = "1"; f.l.input({ isComposing: false });
  eq(padIn(e.relay).filter((m) => m.k !== "ping"), [{ k: "char", ch: "1", mods: ["super"] }], "char");
  eq([f.value, e.W.padKeys.mods.super], ["", "off"], "field cleared, latch off");
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
  f.value = "Hello"; f.l.input({ isComposing: true });
  f.value = "Hello there."; f.l.compositionend();
  eq(padIn(e.relay, "text"), [{ k: "text", text: "Hello there." }], "one text");
  f.value = "Hello there. teh"; f.l.input({ isComposing: false });
  f.value = "Hello there. the "; f.l.input({ isComposing: false });
  const tail = padIn(e.relay).filter((m) => m.k !== "ping").slice(2);
  eq(tail, [{ k: "key", key: "backspace", mods: [] }, { k: "key", key: "backspace", mods: [] }, { k: "text", text: "he " }], "correction");
});

await run("Enter: flushes the text, then presses enter", async (e) => {
  e.W.pad.start();
  const f = field();
  e.W.padKeys.attach(f);
  f.value = "ls"; f.l.input({ isComposing: false });
  f.l.keydown({ key: "Enter", isComposing: false, preventDefault: () => {} });
  eq(padIn(e.relay).filter((m) => m.k !== "ping"), [{ k: "text", text: "ls" }, { k: "key", key: "enter", mods: [] }], "order");
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
  f.value = "c"; f.l.input({ isComposing: false });
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
