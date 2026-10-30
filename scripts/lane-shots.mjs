// Screenshots of F4's honest-status surfaces, with no computer behind the page: every state is
// played through the `?fixture` hook (the template is scripts/multi-shots.mjs). Also checks the
// words on each.
//
//   dx build -p wado-client --platform web
//   node scripts/lane-shots.mjs <out-dir>      (needs Chrome)
//
// States: no picture on the relayed lane (badge open, hint card, NAT cause named); files refused
// on the lane; a big shell dump on a direct link (not blamed on the network); the relay
// unreachable; a seat held by the user's other device with nothing running.

import { spawn } from "node:child_process";
import { mkdirSync, mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const ROOT = new URL("..", import.meta.url).pathname;
const PUB = process.env.WADO_PUB || join(ROOT, "target/dx/wado-client/debug/web/public");
const OUT = process.argv[2] || join(tmpdir(), "wado-lane-shots");
mkdirSync(OUT, { recursive: true });
const T = mkdtempSync(join(tmpdir(), "wado-lane-shots-"));
const HTTP = 8774, CDP = 9374;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const procs = [];
let failures = 0;
const ok = (c, name, d = "") => { if (!c) failures++; console.log(`${c ? "  ok  " : "FAIL  "}${name}${c || !d ? "" : "\n        " + d}`); };
const start = (bin, args) => { const p = spawn(bin, args, { stdio: "ignore" }); procs.push(p); return p; };
const cleanup = () => { for (const p of procs) try { p.kill("SIGTERM"); } catch {} };
for (const sig of ["SIGTERM", "SIGINT"]) process.on(sig, () => { cleanup(); process.exit(1); });

start("python3", ["-m", "http.server", String(HTTP), "--bind", "127.0.0.1", "-d", PUB]);
start("google-chrome-stable", ["--headless=new", "--remote-debugging-port=" + CDP, "--user-data-dir=" + join(T, "chrome"),
  "--no-first-run", "--hide-scrollbars", "about:blank"]);
let list;
for (let i = 0; i < 60 && !list; i++) { await sleep(200); try { list = await (await fetch(`http://127.0.0.1:${CDP}/json/list`)).json(); } catch {} }
const ws = new WebSocket(list.find((t) => t.type === "page").webSocketDebuggerUrl);
await new Promise((r) => (ws.onopen = r));
let id = 0; const pend = new Map();
ws.onmessage = (e) => { const m = JSON.parse(e.data); if (m.id && pend.has(m.id)) { pend.get(m.id)(m); pend.delete(m.id); } };
const cdp = (method, params = {}) => new Promise((r) => { const i = ++id; pend.set(i, r); ws.send(JSON.stringify({ id: i, method, params })); });
const js = async (expr) => (await cdp("Runtime.evaluate", { expression: expr, returnByValue: true, awaitPromise: true })).result?.result?.value;
const until = async (expr, ms = 8000) => { const t = Date.now(); while (Date.now() - t < ms) { if (await js(expr)) return true; await sleep(150); } return false; };
const shot = async (name) => {
  await sleep(2600); // let the page's entrance stagger and the card's motion finish
  const r = await cdp("Page.captureScreenshot", { format: "png" });
  writeFileSync(join(OUT, name + ".png"), Buffer.from(r.result.data, "base64"));
};
const text = (sel) => js(`(document.querySelector(${JSON.stringify(sel)}) || {}).innerText || ""`);

await cdp("Runtime.enable");
await cdp("Page.enable");
await cdp("Emulation.setDeviceMetricsOverride", { width: 412, height: 915, deviceScaleFactor: 2, mobile: true });
const settings = JSON.stringify({ conn_mode: "relay", relay_url: "http://10.255.255.1:9", remote_id: "123456789", profile: 0,
  profiles: [{ name: "bushido", host: "", conn_mode: "relay", relay_url: "http://10.255.255.1:9", remote_id: "123456789" }] });
const BASE = `http://127.0.0.1:${HTTP}/`;
await cdp("Page.navigate", { url: BASE });
await sleep(800);
await js(`localStorage.clear(); localStorage.setItem("wado.settings", ${JSON.stringify(settings)})`);
const fx = (...events) => js(`(() => { const W = window.__wado; ${events.map((e) => `W.fixture(${JSON.stringify(e)});`).join("")} return true; })()`);
const link = (state, extra = {}) => ({ type: "link", state, path: "", rtt_ms: null, net: "wifi", nat: "", turn: false, why: "", cause: "", ...extra });

async function fresh(go = "") {
  await cdp("Page.navigate", { url: BASE + "?fixture" + go });
  await until("!!window.__wado && !!window.__wado.fixture", 10000);
  await sleep(600);
}

try {
  // 1. A session with no direct path: no picture.
  await fresh();
  // The page's own link ticker reads W.pc and W.relayUp every 2 s: set them as a real session would.
  await js(`(() => { const W = window.__wado; W.relayDrop(true); W.relayUp = true; W.sessionOn = true; W.pc = { connectionState: "failed" };
    W.linkNow = Object.assign({}, W.linkNow, { nat: "symmetric" }); W.linkTick(); return true; })()`);
  await fx({ type: "phase", stage: 3 }, { type: "sessionOn" }, link("relayed", { nat: "symmetric" }));
  ok(await until("!!document.querySelector('.lanehint')"), "no picture: the hint card is up");
  const h1 = await text(".lanehint");
  ok(/No picture/.test(h1) && /not wado/.test(h1) && /VPN or a strict NAT/.test(h1) && /Retry direct/.test(h1), "…names the network, the NAT cause, and offers Retry direct", h1);
  ok(/Relayed — limited/.test(await text(".linkbadge")), "the badge says Relayed — limited");
  await js(`document.querySelector('.linkpill').click()`);
  await shot("1-no-picture-relayed");
  const b1 = await text(".linkcard");
  ok(/64 KB\/s/.test(b1) && /not wado/.test(b1), "the badge's card explains the lane", b1);

  // 2. Files refused on the lane, in the computer's own words.
  await fresh();
  await fx({ type: "phase", stage: 2 }, link("relayed"),
    { type: "laneHint", spot: "files", on: true, detail: "2.0 MB is more than the relay's limited lane carries (files up to 1 MB go without a direct connection). This is the network, not wado — Retry direct, or switch off a VPN or use mobile data." });
  ok(await until("!!document.querySelector('.lanehint[data-spot=files]')"), "files: the hint card is up");
  await shot("2-files-refused-on-lane");

  // 3. A big shell dump on a direct link: the relay's pace, not the network's fault.
  await fresh();
  await js(`(() => { const W = window.__wado; W.relayDrop(true); W.relayUp = true; W.sessionOn = true;
    W.pc = { connectionState: "connected", getStats: async () => new Map() }; W.linkTick(); return true; })()`);
  await fx({ type: "phase", stage: 4 }, { type: "sessionOn" }, link("direct", { path: "lan", rtt_ms: 4 }),
    { type: "laneHint", spot: "shell", on: true, detail: "" });
  ok(await until("!!document.querySelector('.lanehint[data-spot=shell]')"), "shell: the hint card is up");
  const h3 = await text(".lanehint");
  await sleep(2600);
  ok(/Direct/.test(await text(".linkbadge")) || !(await text(".linkbadge")), "…the badge still says direct after the page's own tick");
  ok(/relay's pace/.test(h3) && !/not wado/.test(h3) && !/Retry direct/.test(h3), "…on a direct link it blames nobody and offers no retry", h3);
  await shot("3-shell-dump-direct");

  // 4. The relay is unreachable, in a session.
  await fresh();
  await fx({ type: "phase", stage: 3 }, { type: "sessionOn" }, link("offline", { why: "relay_unreachable" }));
  await until("!!document.querySelector('.linkpill')");
  await js(`document.querySelector('.linkpill').click()`);
  const b4 = await text(".linkcard");
  ok(/wado's relay is unreachable — not your computer/.test(b4), "relay unreachable: the plan's words", b4);
  await shot("4-relay-unreachable");

  // 5. The seat held by this user's other device, nothing running: no red error, no "0 sessions".
  await fresh("&go=session");
  await until("!!document.querySelector('#computer')", 10000);
  await js(`window.__wado.sessionsWatch = () => {}; window.__wado.sessionsGet = () => {};`);
  // As relay_link.js does on the refusal, so the page's own link ticker agrees with it.
  await js(`window.__wado._relaySeatBusy = { holder: "Android · Chrome", idle: true }; window.__wado.linkTick(); true`);
  await fx({ type: "phase", stage: 1, error: "" }, { type: "seatBusy", holder: "Android · Chrome", idle: true },
    link("waiting", { why: "your other device, Android · Chrome, is connected — nothing is running there" }));
  await sleep(500);
  const page5 = await text("#computer");
  ok(/Android · Chrome is connected, nothing running/.test(page5), "the Desktop door says who has it and that nothing runs there", page5.slice(0, 600));
  ok(!/every wado session/.test(page5) && !/take over/.test(page5), "…no 'every session is in use', no take-over badge");
  ok(!/nothing to reach/.test(page5) && /Free/.test(page5), "…and the link reads Free, not None", page5.slice(0, 300));
  await shot("5-idle-seat-other-device");

  // 6. …and when something does run there, sessions are not claimed to be 0.
  await js(`window.__wado._relaySeatBusy = { holder: "Android · Chrome", idle: false }; window.__wado.linkTick(); true`);
  await fx({ type: "seatBusy", holder: "Android · Chrome", idle: false });
  await sleep(500);
  const page6 = await text("#computer");
  ok(/running on another device/.test(page6) && /In use on another device/.test(page6), "busy: sessions say running elsewhere, not 0", page6.slice(0, 600));
  await shot("6-in-use-other-device");
} catch (e) {
  failures++;
  console.log("FAIL " + (e.stack || e));
} finally {
  cleanup();
}
console.log(`screenshots: ${OUT}`);
console.log(failures ? `\n${failures} failed` : "\nall lane shots taken");
process.exit(failures ? 1 : 0);
