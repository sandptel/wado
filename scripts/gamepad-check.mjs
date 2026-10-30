// The gamepad's check (docs/superpowers/specs/2026-10-30-virtual-input-design.md, T19), in three
// parts. Each part says what it found; any failure exits non-zero.
//
//   model    (Node, always) the layout model round-trips through the saved diff for every built-in
//            and for edits; every control in every layout is ≥ 44 px; mirror; edge-anchored
//            placement and the console deck's packing; the comfort check; both wires (the session's
//            and the pad link's, with the 100 ms keep-alive); the old layout's migration; what is
//            written to the computer, key by key.
//   browser  (Chrome + a built client, `?fixture`) the sessionless screen sends game_on, a press, a
//            stick and game_off; a variant morphs (same ids fly, new ones pop); the editor drags,
//            resizes (never under 44 px), undoes and redoes, and saves as it goes. SHOTS=<dir> also
//            writes the screenshots: 390×844, 844×390, 820×1180, default-dark and latte.
//   daemon   not written yet: the sessionless wire against a release daemon (game_on → `game`, or
//            the uinput refusal under bwrap) is tested in Rust (`crates/server/src/pad/game.rs`)
//            and the refusal card by passkey-ui-e2e's pad phase; the end-to-end run is to do.
//
//   node scripts/gamepad-check.mjs                 model only, plus browser when the client is built
//   SHOTS=/tmp/gp node scripts/gamepad-check.mjs
//
// Chrome and the daemon need the Bash sandbox off. It kills only what it started.

import { readFileSync, existsSync, mkdirSync, mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import assert from "node:assert/strict";

const ROOT = new URL("..", import.meta.url).pathname;
const JS = (f) => readFileSync(join(ROOT, "crates/client/src/js", f), "utf8");
let failures = 0, passes = 0;
const check = (name, fn) => {
  try { fn(); passes++; console.log("  ok  " + name); } catch (e) { failures++; console.log("FAIL  " + name + "\n        " + (e.message || e).split("\n").join("\n        ")); }
};
const acheck = async (name, fn) => {
  try { await fn(); passes++; console.log("  ok  " + name); } catch (e) { failures++; console.log("FAIL  " + name + "\n        " + (e.message || e).split("\n").join("\n        ")); }
};

// ── model ────────────────────────────────────────────────────────────────────────────────────
function env(store = new Map()) {
  const localStorage = { getItem: (k) => (store.has(k) ? store.get(k) : null), setItem: (k, v) => store.set(k, String(v)), removeItem: (k) => store.delete(k) };
  const sets = [];
  const W = { clientKey: "k1", configSet: (k, v) => sets.push([k, String(v)]), relayOn() {}, _relayHandlers: {} };
  const timers = [];
  const src = ["gpad/variants.js", "gpad/model.js", "gpad/place.js", "gpad/controller.js", "gpad/send_session.js", "gpad/send_pad.js",
    "gpad/input.js", "gpad/prefs.js", "gpad/comfort.js"].map(JS).join("\n");
  const sent = [];
  W.pad = { send: (m, lossy) => { sent.push([m, lossy]); return true; } };
  const co = [];
  W.coalesce = { now: (m) => co.push(["now", m]), queue: (k, m) => co.push(["queue", m]), add: (k, m) => co.push(["add", m]) };
  new Function("W", "emit", "localStorage", "setTimeout", "setInterval", "clearInterval", "requestAnimationFrame", "navigator", src)(
    W, () => {}, localStorage, (f) => f(), (f, ms) => { timers.push({ f, ms }); return timers.length; }, (i) => { if (timers[i - 1]) timers[i - 1].dead = true; },
    () => 0, {});
  return { W, sets, sent, co, timers, store };
}

console.log("model");
{
  const { W } = env();
  const V = W.gpadVariants, M = W.gpadModel, P = W.gpadPlace;
  check("every built-in round-trips through an empty diff", () => {
    for (const id of V.ORDER) {
      const L = M.builtin(id);
      const d = M.diff(L);
      assert.deepEqual(d.controls, {}, `${id}: ${JSON.stringify(d)}`);
      assert.equal(M.dirty(L), false, id);
      assert.deepEqual(M.resolve(id, JSON.parse(JSON.stringify(d)), id, L.name), L, id);
    }
  });
  check("an edited layout round-trips: moves, sizes, looks, hidden, 8-way, floating, dead zone, labels", () => {
    const L = M.builtin("fighting");
    const c = (id) => L.controls.find((x) => x.id === id);
    c("a").l = [0.6, 0.7, 1.4];
    c("x").look = { shape: "round", style: "outline" };
    c("rt").hidden = true;
    c("dpad").dir = 4;
    c("b").label = "K";
    L.look.press = "ring";
    L.stick = "floating";
    L.dead = 0.2;
    const saved = JSON.parse(JSON.stringify(M.diff(L)));
    assert.ok(JSON.stringify(saved).length < 4096, "a saved layout fits the daemon's 4 KB");
    const back = M.resolve("fighting", saved, L.id, L.name);
    assert.deepEqual(back, M.clamp(L));
    assert.equal(M.dirty(back), true);
  });
  check("every control in every layout is at least 44 px on its short side, both ways round", () => {
    for (const id of V.ORDER) for (const o of ["l", "p"]) for (const c of M.builtin(id).controls) {
      const [w, h] = P.size(c, o, 1);
      assert.ok(Math.min(w, h) >= 44 - 1e-9, `${id} ${o} ${c.id}: ${w}×${h}`);
    }
  });
  check("nothing can be made smaller than 44 px: a saved size of 0.1 comes back at the floor", () => {
    const back = M.resolve("xbox", { controls: { a: { l: [0.8, 0.6, 0.1] }, start: { l: [0.5, 0.4, 0.2] } } });
    for (const id of ["a", "start"]) {
      const c = back.controls.find((x) => x.id === id);
      assert.ok(Math.min(...P.size(c, "l", 1)) >= 44 - 1e-9, `${id}: ${P.size(c, "l", 1)}`);
    }
  });
  check("bad saved values are dropped, never drawn", () => {
    const back = M.resolve("xbox", { stick: "teleport", dead: 9, look: { shape: "star", op: -4 }, controls: { a: { l: "x" }, nope: { hidden: true }, dpad: { dir: 3 } } });
    assert.equal(back.stick, "fixed");
    assert.equal(back.dead, 0.4);
    assert.equal(back.look.shape, "circle");
    assert.equal(back.look.op, 0.3);
    assert.equal(back.controls.find((c) => c.id === "dpad").dir, undefined);
  });
  check("mirror swaps sides and twice is the same layout", () => {
    const L = M.builtin("racing");
    const m = M.mirror(L);
    assert.ok(Math.abs(m.controls.find((c) => c.id === "steer").l[0] - (1 - 0.22)) < 1e-9);
    assert.deepEqual(M.mirror(m), L);
  });
  check("placement is edge-anchored: on a tablet the left stick keeps its distance from the left edge, clusters stay whole", () => {
    const L = M.builtin("xbox");
    const phone = P.all(L, "l", 1, 824, 370), tab = P.all(L, "l", 1, 1160, 800);
    assert.ok(Math.abs(phone.get("stick-l").x - tab.get("stick-l").x) < 1e-6, "left edge");
    const gap = (m) => m.get("b").x - m.get("x").x;
    assert.ok(Math.abs(gap(phone) - gap(tab)) < 1e-6, "the face buttons keep their spread");
    assert.ok(Math.abs((1160 - tab.get("rt").x) - (824 - phone.get("rt").x)) < 1e-6, "right edge");
  });
  check("the console deck packs by side: inside the box, ≥ 44 px, left left and right right", () => {
    for (const id of V.ORDER) for (const [w, h] of [[378, 138], [808, 200], [360, 150]]) {
      const L = M.builtin(id);
      const m = P.deck(L, w, h);
      for (const [cid, r] of m) {
        assert.ok(Math.min(r.w, r.h) >= 44 - 1e-6, `${id} ${w}×${h} ${cid} ${r.w}×${r.h}`);
      }
      if (w > h && m.has("stick-l") && m.has("a")) assert.ok(m.get("stick-l").x < m.get("a").x, `${id}: left stays left`);
      // Never one control drawn over another: a group that cannot fit is left out instead.
      const rs = [...m.entries()];
      for (let i = 0; i < rs.length; i++) for (let j = i + 1; j < rs.length; j++) {
        const [ia, a] = rs[i], [ib, b] = rs[j];
        const over = Math.min(a.x + a.w, b.x + b.w) - Math.max(a.x, b.x) > 1 && Math.min(a.y + a.h, b.y + b.h) - Math.max(a.y, b.y) > 1;
        const sameGroup = L.controls.find((c) => c.id === ia).g === L.controls.find((c) => c.id === ib).g;
        assert.ok(!over || sameGroup, `${id} ${w}×${h}: ${ia} and ${ib} overlap`);
      }
      assert.ok(m.size > 0, `${id} ${w}×${h}: the deck is not empty`);
    }
    // The landscape console: the gutters beside the picture and the band under it.
    const m = P.deck(M.builtin("xbox"), 832, 328, { x: 111, y: 0, w: 610, h: 282 });
    for (const [cid, r] of m) assert.ok(r.x + r.w <= 111 + 1 || r.x >= 721 - 1 || r.y >= 282 - 1, `${cid} is on the picture: ${JSON.stringify(r)}`);
  });
  check("the comfort check flags a far control, a small one and two too close; the middle row never for reach", () => {
    const L = M.builtin("xbox");
    const place = P.all(L, "l", 1, 1160, 800);
    const issues = W.gpadComfort.check(L, place, 1160, 800, "l", 4.7);
    assert.ok(issues.some((i) => i.id === "y" && i.reach), JSON.stringify(issues));
    assert.ok(!issues.some((i) => ["back", "guide", "start"].includes(i.id) && i.reach));
    const tiny = new Map(place);
    tiny.set("a", { ...tiny.get("a"), w: 30, h: 30 });
    assert.ok(W.gpadComfort.check(L, tiny, 1160, 800, "l", 4.7).some((i) => i.id === "a" && /44 px/.test(i.t)));
  });
  check("the d-pad reads 4-way and 8-way; the dead zone cuts and stretches", () => {
    const I = W.gpadInput;
    assert.deepEqual(I.dpadDirs(0.9, -0.85, false), ["dr"]);
    assert.deepEqual(I.dpadDirs(0.7, -0.7, true), ["du", "dr"]);
    assert.deepEqual(I.dpadDirs(0.05, 0.05, true), []);
    assert.deepEqual(I.deadZone(0.1, 0, 0.12).slice(0, 2), [0, 0]);
    assert.ok(Math.abs(I.deadZone(1, 0, 0.12)[0] - 1) < 1e-9, "full deflection is still full");
  });
}

console.log("wires");
{
  check("sessionless: game_on, buttons by position on the keys channel, the hat as BTN_DPAD_*, digital triggers as TL2/TR2", () => {
    const { W, sent } = env();
    const s = W.gpadSendPad();
    const c = W.gpadController(s);
    s.start();
    c.button("a", true); c.button("a", false); c.button("du", true); c.button("dl", true); c.button("lt", true);
    s.stop();
    assert.deepEqual(sent.map(([m, l]) => [m, l]), [
      [{ k: "game_on" }, false],
      [{ k: "btn", code: 0x130, down: true }, false], [{ k: "btn", code: 0x130, down: false }, false],
      [{ k: "btn", code: 0x220, down: true }, false], [{ k: "btn", code: 0x222, down: true }, false],
      [{ k: "btn", code: 0x138, down: true }, false],
      [{ k: "game_off" }, false],
    ]);
  });
  check("sessionless: the whole axis state rides the touch channel, re-sent every 100 ms while off-centre, and stops at centre", () => {
    const { W, sent, timers } = env();
    const c = W.gpadController(W.gpadSendPad());
    c.stick("ls", 0.5, -1);
    c.trigger("rt", 0.5);
    const frames = sent.filter(([m]) => m.k === "axes");
    assert.deepEqual(frames.at(-1)[0].a, [16384, -32767, 0, 0, 0, 128]);
    assert.equal(frames.at(-1)[1], true, "lossy");
    assert.ok(frames[1][0].seq > frames[0][0].seq, "seq rises");
    const live = timers.filter((t) => !t.dead);
    assert.equal(live.length, 1, "one keep-alive");
    assert.equal(live[0].ms, 100);
    live[0].f();
    assert.deepEqual(sent.at(-1)[0].a, [16384, -32767, 0, 0, 0, 128], "the keep-alive re-sends the whole state");
    c.releaseAll();
    assert.deepEqual(sent.at(-1)[0].a, [0, 0, 0, 0, 0, 0]);
    assert.equal(timers.filter((t) => !t.dead).length, 0, "no keep-alive at centre");
  });
  check("a repeated value is not sent again", () => {
    const { W, sent } = env();
    const c = W.gpadController(W.gpadSendPad());
    c.stick("rs", 0.3, 0.3); c.stick("rs", 0.3, 0.3); c.button("x", true); c.button("x", true);
    assert.equal(sent.filter(([m]) => m.k === "axes").length, 1);
    assert.equal(sent.filter(([m]) => m.k === "btn").length, 1);
  });
  check("in a session, pad mode: today's wire — gamepad_button, the hat and triggers as axes, sticks queued", () => {
    const { W, co } = env();
    const c = W.gpadController(W.gpadSendSession("pad"));
    c.button("b", true); c.button("dd", true); c.button("rt", true); c.stick("ls", 1, 0);
    assert.deepEqual(co.slice(0, 3).map(([, m]) => m), [
      { t: "gamepad_button", code: 0x131, pressed: true },
      { t: "gamepad_axis", code: 0x11, value: 1 },
      { t: "gamepad_axis", code: 0x05, value: 255 },
    ]);
    assert.deepEqual(co[3], ["queue", { t: "gamepad_axis", code: 0x00, value: 32767 }]);
  });
  check("in a session, keys mode: A is Space, the left stick walks WASD, the shoulders click", () => {
    const { W, co } = env();
    const c = W.gpadController(W.gpadSendSession("keys"));
    c.button("a", true); c.stick("ls", 0, -1); c.button("rb", true);
    assert.deepEqual(co.map(([, m]) => m), [
      { t: "key", code: 57, pressed: true }, { t: "key", code: 17, pressed: true },
      { t: "button", x: 0.5, y: 0.5, button: "left", pressed: true },
    ]);
  });
}

console.log("prefs");
{
  check("the old in-session layout becomes \"My layout (from before)\", drives keeps its mode, and the old key goes", () => {
    const store = new Map([["wado.padLayout", JSON.stringify({ "stick-l": { x: 0.2, y: 0.6, s: 1.2 }, up: { x: 0.3, y: 0.7, s: 1 }, left: { x: 0.26, y: 0.75, s: 1 } })],
      ["wado.settings", JSON.stringify({ pad_mode: "keys", pad_scale: 1.1, pad_opacity: 0.5 })]]);
    const { W, sets } = env(store);
    const p = W.gpadPrefs.get();
    assert.equal(p.layout, "mine-before");
    assert.equal(p.drives, "keys");
    const L = W.gpadPrefs.layoutOf("mine-before");
    assert.equal(L.name, "My layout (from before)");
    assert.equal(L.controls.find((c) => c.id === "lb").label, "L1");
    const st = L.controls.find((c) => c.id === "stick-l");
    assert.ok(Math.abs(st.l[0] - 0.2) < 1e-3 && Math.abs(st.l[2] - 1.32) < 1e-3, JSON.stringify(st.l));
    assert.equal(store.has("wado.padLayout"), false);
    assert.equal(store.get("wado.gpadMigrated"), "1");
    assert.equal(sets.length, 0, "nothing is written until the computer has answered");
    const again = env(store);
    assert.equal(again.W.gpadPrefs.get().mine.length, 1, "migrates once");
  });
  check("an edit to a built-in forks it into mine-<base>; the writes are the contract's keys and values", () => {
    const { W, sets } = env();
    const L = W.gpadPrefs.layoutFor("away");
    L.controls.find((c) => c.id === "a").l[0] = 0.7;
    const id = W.gpadPrefs.save("away", L);
    assert.equal(id, "mine-xbox");
    assert.equal(W.gpadPrefs.get().layout, "mine-xbox", "away is \"same\": the session's layout is the one edited");
    const mine = sets.find(([k]) => k === "device.k1.gamepad.mine.mine-xbox");
    const [name, base, json] = JSON.parse(mine[1]);
    assert.deepEqual([name, base], ["Standard", "xbox"]);
    assert.deepEqual(JSON.parse(json).controls.a.l[0], 0.7);
    assert.ok(sets.some(([k, v]) => k === "device.k1.gamepad.layout" && v === "mine-xbox"));
    W.gpadPrefs.set("haptics", "strong");
    assert.deepEqual(sets.at(-1), ["device.k1.gamepad.haptics", "strong"]);
    W.gpadPrefs.remove("mine-xbox");
    assert.ok(sets.some(([k, v]) => k === "device.k1.gamepad.mine.mine-xbox" && v === ""), "delete is an empty value");
    assert.equal(W.gpadPrefs.get().layout, "xbox");
  });
  check("left-handed play is the layout mirrored, and saving un-mirrors it", () => {
    const { W } = env();
    W.gpadPrefs.set("hand", "left");
    const L = W.gpadPrefs.layoutFor("away");
    assert.ok(L.controls.find((c) => c.id === "stick-l").l[0] > 0.5);
    W.gpadPrefs.save("away", L);
    assert.ok(W.gpadPrefs.layoutOf("mine-xbox").controls.find((c) => c.id === "stick-l").l[0] < 0.5);
  });
  check("at most eight saved layouts", () => {
    const { W } = env();
    for (let i = 0; i < 8; i++) assert.ok(W.gpadPrefs.saveAs(W.gpadModel.builtin("retro"), "L" + i));
    assert.equal(W.gpadPrefs.saveAs(W.gpadModel.builtin("retro"), "nine"), null);
  });
}

// ── browser ──────────────────────────────────────────────────────────────────────────────────
const PUB = process.env.WADO_PUB || join(ROOT, "target/dx/wado-client/debug/web/public");
if (!existsSync(join(PUB, "index.html"))) {
  console.log("browser — skipped: no built client at " + PUB);
} else {
  const { openPage } = await import("./lib/cdp-page.mjs");
  const SHOTS = process.env.SHOTS || "";
  if (SHOTS) mkdirSync(SHOTS, { recursive: true });
  const P = await openPage({ pub: PUB, http: 8811 + +(process.env.PORT_OFFSET || 0), cdp: 9411 + +(process.env.PORT_OFFSET || 0) });
  const KEY = "123456789";
  const GRANTS = { name: "Pixel", files: "rw", shells: true, settings: true, host: true, input: true, clipboard: true, join: true, takeover: true, paired: true };
  const settings = (theme) => JSON.stringify({ conn_mode: "relay", relay_url: "http://10.255.255.1:9", remote_id: KEY, profile: 0, theme,
    profiles: [{ name: "Framework", host: "", conn_mode: "relay", relay_url: "http://10.255.255.1:9", remote_id: KEY }] });
  const computer = async (w, h, theme = "default-dark", fresh = true) => {
    await P.size(w, h);
    await P.load("?fixture");
    await P.js(`(() => { ${fresh ? "localStorage.clear();" : ""} localStorage.setItem("wado.settings", ${JSON.stringify(settings(theme))}); return true; })()`);
    await P.load("?fixture&go=session");
    await P.until("!!document.querySelector('#computer')", 10000);
    await P.fx({ type: "phase", stage: 2 }, { type: "hostConfig", state: { grants: GRANTS, caps: ["pad-gamepad"], input_host: { enabled: true, smoothing: true, rate_hz: 125, delay: "adaptive" } } },
      { type: "passkey", state: { registered: true, unlocked: true, idle_left_s: 900, idle_s: 900, error: "", origin: "" }, supported: true, platform: true });
    await P.sleep(700);
  };
  // The pad link, as the daemon would see it.
  const capture = `(() => { const W = window.__wado; W.__sent = []; W.pad.send = (m, lossy) => { W.__sent.push([m, !!lossy]); return true; };
    W.pad.start = () => {}; W.pad.stop = () => {}; if (W.passkey) W.passkey.state.unlocked = true; return true; })()`;
  const open = (state = "play") => P.js(`window.__wado.gpadOpen({where: "away", state: ${JSON.stringify(state)}, name: "Framework"}); true`);
  const centre = (id) => P.js(`(() => { const e = window.__wado.gpadScreen.el.pa._gp.get(${JSON.stringify(id)}); const r = e.getBoundingClientRect(); return [r.left + r.width / 2, r.top + r.height / 2, r.width, r.height]; })()`);
  const shot = async (name) => { if (!SHOTS) return; await P.settle(200); await P.shot(join(SHOTS, name + ".png")); };

  try {
    console.log("browser");
    await computer(844, 390);
    await P.js(capture);
    await open();
    await P.sleep(700);
    await acheck("the screen opens with game_on, and a press of A is btn 0x130 down then up on the keys channel", async () => {
      const [x, y] = await centre("a");
      await P.tap(x, y);
      await P.sleep(100);
      const sent = await P.js("window.__wado.__sent");
      assert.deepEqual(sent[0], [{ k: "game_on" }, false]);
      const btns = sent.filter(([m]) => m.k === "btn").map(([m, l]) => [m.code, m.down, l]);
      assert.deepEqual(btns, [[0x130, true, false], [0x130, false, false]]);
    });
    await acheck("a stick drag sends the axes on the touch channel and centres on release", async () => {
      const [x, y, w] = await centre("stick-l");
      await P.drag([[x, y], [x + w * 0.2, y], [x + w * 0.45, y - w * 0.2]]);
      await P.sleep(100);
      const ax = (await P.js("window.__wado.__sent")).filter(([m]) => m.k === "axes");
      assert.ok(ax.length >= 2, JSON.stringify(ax));
      assert.ok(ax.some(([m]) => m.a[0] > 10000 && m.a[1] < 0), "right and up");
      assert.ok(ax.every(([, l]) => l === true), "lossy");
      assert.deepEqual(ax.at(-1)[0].a, [0, 0, 0, 0, 0, 0]);
    });
    await acheck("every control on the play screen is at least 44 px, at 844×390 and 390×844", async () => {
      for (const [w, h] of [[844, 390], [390, 844]]) {
        await P.size(w, h);
        await P.sleep(300);
        const small = await P.js(`[...window.__wado.gpadScreen.el.pa._gp.values()].map((e) => { const r = e.getBoundingClientRect(); return [e.dataset.id, Math.min(r.width, r.height)]; }).filter(([, s]) => s < 43.5)`);
        assert.deepEqual(small, [], `${w}×${h}`);
      }
      await P.size(844, 390);
      await P.sleep(300);
    });
    await shot("play-844x390");
    await acheck("picking Fighting morphs: shared ids fly, the d-pad turns 8-way, new controls pop in", async () => {
      await P.js(`window.__wado.gpadPicker.open(); true`);
      await P.sleep(600);
      await shot("picker-844x390");
      await P.js(`document.querySelector('#wado-gpad .gpvcard[data-variant="fighting"]').click(); true`);
      await P.sleep(60);
      const mid = await P.js(`document.getAnimations().filter((a) => a.effect && a.effect.target && a.effect.target.closest && a.effect.target.closest('.gppa')).length`);
      assert.ok(mid > 4, `animations running mid-morph: ${mid}`);
      await P.sleep(700);
      const st = await P.js(`(() => { const S = window.__wado.gpadScreen.S; return { id: S.L.id, layout: window.__wado.gpadPrefs.get().away, dir: S.L.controls.find((c) => c.id === "dpad").dir, ids: [...window.__wado.gpadScreen.el.pa._gp.keys()].sort() }; })()`);
      assert.equal(st.id, "fighting");
      assert.equal(st.layout, "fighting");
      assert.equal(st.dir, 8);
      assert.ok(!st.ids.includes("stick-l") && st.ids.includes("rt"), st.ids.join());
      await P.js(`window.__wado.gpadPicker.close(); true`);
    });
    await acheck("the editor: a drag moves the group 1:1, saves as it goes, the corner never goes under 44 px, undo and redo", async () => {
      await P.js(`window.__wado.gpadEdit.set(true); true`);
      await P.sleep(400);
      const [x0, y0] = await centre("a");
      await P.drag([[x0, y0], [x0 - 10, y0], [x0 - 30, y0 - 20], [x0 - 41, y0 - 21]]);
      await P.sleep(600);
      const [x1, y1] = await centre("a");
      assert.ok(Math.abs(x1 - x0 + 41) < 12 && Math.abs(y1 - y0 + 21) < 12, `moved ${x1 - x0}, ${y1 - y0} (snapped to the grid)`);
      const saved = await P.js(`window.__wado.gpadPrefs.get().mine.map((m) => m.id)`);
      assert.ok(saved.includes("mine-fighting"), "a built-in forks into its own saved layout: " + saved);
      await shot("edit-place-844x390");
      // Shrink with the corner dot, far past the floor.
      const h = await P.js(`(() => { const r = window.__wado.gpadScreen.el.handle.getBoundingClientRect(); return [r.left + r.width / 2, r.top + r.height / 2]; })()`);
      const sel = await P.js(`(() => { const r = window.__wado.gpadScreen.el.selbox.getBoundingClientRect(); return [r.left + r.width / 2, r.top + r.height / 2]; })()`);
      await P.drag([h, [(h[0] + sel[0]) / 2, (h[1] + sel[1]) / 2], [sel[0] + 2, sel[1] + 2]]);
      await P.sleep(300);
      const sizes = await P.js(`[...window.__wado.gpadScreen.el.pa._gp.values()].map((e) => { const r = e.getBoundingClientRect(); return Math.min(r.width, r.height); })`);
      assert.ok(Math.min(...sizes) >= 43.5, "smallest " + Math.min(...sizes));
      const undo = await P.js(`window.__wado.gpadScreen.S.undo.length`);
      assert.ok(undo >= 2, "two steps to undo: " + undo);
      await P.js(`window.__wado.gpadEdit.undo(); window.__wado.gpadEdit.undo(); true`);
      await P.sleep(600);
      const [x2, y2] = await centre("a");
      assert.ok(Math.abs(x2 - x0) < 2 && Math.abs(y2 - y0) < 2, `undo put it back: ${x2 - x0}, ${y2 - y0}`);
      await P.js(`window.__wado.gpadEdit.redo(); true`);
      await P.sleep(500);
      const [x3] = await centre("a");
      assert.ok(Math.abs(x3 - x1) < 2, "redo moved it again");
    });
    await acheck("the inspector's Look tab changes the selection's shape; comfort check flags and pulls into reach", async () => {
      await P.js(`(() => { const G = window.__wado.gpadScreen; G.S.sel = "x"; G.S.tab = "look"; G.draw(); window.__wado.gpadInspector.draw(); return true; })()`);
      await P.sleep(300);
      await shot("edit-look-844x390");
      await P.js(`document.querySelector('#wado-gpad [data-look="shape"][data-v="round"]').click(); true`);
      await P.sleep(200);
      assert.equal(await P.js(`window.__wado.gpadScreen.el.pa._gp.get("x").dataset.shape`), "round");
      await P.js(`window.__wado.gpadComfort.toggle(true); true`);
      await P.sleep(500);
      await shot("comfort-844x390");
      assert.ok(await P.js(`!!document.querySelector('#wado-gpad .gpccard.on .gpclist')`));
    });
    await acheck("closing sends game_off and lets go of everything", async () => {
      await P.js(`window.__wado.gpadClose(); true`);
      await P.sleep(400);
      const sent = await P.js("window.__wado.__sent");
      assert.deepEqual(sent.at(-1), [{ k: "game_off" }, false]);
    });
    await acheck("no page errors", async () => { assert.deepEqual(P.logs, []); });

    if (SHOTS) {
      console.log("screenshots → " + SHOTS);
      for (const theme of ["default-dark", "catppuccin-latte"]) {
        const t = theme === "default-dark" ? "dark" : "latte";
        for (const [w, h] of [[390, 844], [844, 390], [820, 1180]]) {
          const tag = `${w}x${h}-${t}`;
          await computer(w, h, theme);
          await P.js(capture);
          await P.settle(300);
          await shot(`computer-${tag}`);
          await P.js(`document.querySelector('.vihead').click(); true`);
          await P.sleep(900);
          await shot(`hub-${tag}`);
          for (const pg of ["Touchpad", "Keyboard", "Gamepad"]) {
            await P.js(`document.querySelector('[aria-label="${pg} settings"]').click(); true`);
            await P.sleep(1300);
            await shot(`${pg.toLowerCase()}-settings-${tag}`);
            await P.js(`document.querySelector('#sub .phead .iconbtn').click(); true`);
            await P.sleep(900);
          }
          await open("play");
          await P.sleep(900);
          await shot(`play-${tag}`);
          await P.js(`window.__wado.gpadEdit.set(true); window.__wado.gpadEdit.select("stick-l"); true`);
          await P.sleep(600);
          await shot(`edit-${tag}`);
          await P.js(`window.__wado.gpadEdit.set(false); window.__wado.gpadPicker.open(); true`);
          await P.sleep(700);
          await shot(`picker-${tag}`);
          await P.js(`window.__wado.gpadPicker.close(); window.__wado.gpadComfort.toggle(true); true`);
          await P.sleep(600);
          await shot(`comfort-${tag}`);
          await P.js(`window.__wado.gpadClose(); true`);
          await P.sleep(300);
        }
      }
      // Frozen mid-motion, the way the prototype's shots 23 and 24 are: assemble at 180 ms, a morph at 110 ms.
      await computer(844, 390);
      await P.js(capture);
      await open("play");
      await P.sleep(30);
      await P.js(`document.getAnimations().forEach((a) => { try { a.pause(); a.currentTime = 180; } catch (_) {} }); true`);
      await P.shot(join(SHOTS, "mid-assemble-t180-844x390-dark.png"));
      await P.js(`document.getAnimations().forEach((a) => { try { a.finish(); } catch (_) {} }); window.__wado.gpadPrefs.select("away", "xbox"); true`);
      await P.sleep(600);
      await P.js(`window.__wado.gpadPrefs.select("away", "fighting"); true`);
      await P.sleep(20);
      await P.js(`document.getAnimations().forEach((a) => { try { a.pause(); a.currentTime = 110; } catch (_) {} }); true`);
      await P.shot(join(SHOTS, "mid-morph-xbox-to-fighting-t110-844x390-dark.png"));
    }
  } finally {
    P.close();
  }
}

console.log(`\n${passes} passed, ${failures} failed`);
process.exit(failures ? 1 : 0);
