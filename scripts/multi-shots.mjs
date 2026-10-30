// Screenshots of the Computer page's Sessions section (Decision Log 2026-10-30), with no computer
// behind the page: every state is played through the `?fixture` hook. Also checks that each row
// state offers what it should.
//
//   dx build -p wado-client --platform web
//   node scripts/multi-shots.mjs <out-dir>      (needs Chrome)
//
// States: three in one session with a lead; full; an older daemon; an older relay; the speed test
// measuring, good, worse, refused; a take-over confirm naming who is dropped; the join toast;
// ghost pointers and the participants chip over the picture.

import { spawn } from "node:child_process";
import { mkdirSync, mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const ROOT = new URL("..", import.meta.url).pathname;
const PUB = process.env.WADO_PUB || join(ROOT, "target/dx/wado-client/debug/web/public");
const OUT = process.argv[2] || join(tmpdir(), "wado-multi-shots");
mkdirSync(OUT, { recursive: true });
const T = mkdtempSync(join(tmpdir(), "wado-multi-shots-"));
const HTTP = 8771, CDP = 9371;
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
const shot = async (name, scrollTo) => {
  if (scrollTo) await js(`document.querySelector(${JSON.stringify(scrollTo)})?.scrollIntoView({block:"start"}); document.querySelector("#computer")?.scrollBy(0,-70)`);
  await sleep(1400); // let the motion settle: rows, chips, the meter
  const r = await cdp("Page.captureScreenshot", { format: "png" });
  writeFileSync(join(OUT, name + ".png"), Buffer.from(r.result.data, "base64"));
};

await cdp("Runtime.enable");
await cdp("Page.enable");
await cdp("Emulation.setDeviceMetricsOverride", { width: 412, height: 915, deviceScaleFactor: 2, mobile: true });
const settings = JSON.stringify({ conn_mode: "relay", relay_url: "http://10.255.255.1:9", remote_id: "123456789", profile: 0,
  profiles: [{ name: "Desk", host: "", conn_mode: "relay", relay_url: "http://10.255.255.1:9", remote_id: "123456789" }] });
const BASE = `http://127.0.0.1:${HTTP}/`;
await cdp("Page.navigate", { url: BASE });
await sleep(800);
await js(`localStorage.clear(); localStorage.setItem("wado.settings", ${JSON.stringify(settings)})`);

const P = (id, name, hue, lead, link = "") => ({ id, name, hue, lead, since_ms: 1e12 + id, link });
const S = (n, w, h, apps, participants, max, extra = {}) => ({ instance: "123456789:" + n, started_ms: 1, age_s: 1300, width: w, height: h, fps: 60,
  apps, viewer: participants[0] ? participants[0].name : null, detached: false, lead: participants.find((p) => p.lead)?.name || null,
  participants, max_participants: max, kbps: 6000, ...extra });
const verdict = (level, down, reasons, max = 4) => ({ level, reasons, down_kbps: down, rtt_ms: 28, suggested_max: max, max_participants: max,
  participants: 2, session_kbps: 6000, host: { mem_avail_mb: 9000, load_per_core: 0.3, encoder: "hardware", sessions: 2, max_sessions: 4, uplink_kbps: down } });
const caps = ["park", "hold", "takeover", "gate", "ping", "leave", "seat_hold", "occupancy", "control", "room_strict"];
const sessions = [
  S(1, 1080, 2400, ["org.mozilla.firefox", "foot"], [P(3, "Pixel 8 · Chrome", 3, true, "direct · lan · 4 ms"), P(7, "Laptop · Firefox", 5, false), P(9, "iPad · Safari", 1, false)], 4),
  S(2, 1920, 1080, ["code", "org.gnome.Nautilus"], [P(2, "Desk · Chrome", 6, true), P(4, "Phone · Chrome", 2, false), P(5, "Tab · Chrome", 0, false), P(6, "TV · Chrome", 4, false)], 4),
];
const fx = (...events) => js(`(() => { const W = window.__wado; ${events.map((e) => `W.fixture(${JSON.stringify(e)});`).join("")} return true; })()`);

async function open() {
  await cdp("Page.navigate", { url: BASE + "?fixture&go=session" });
  await until("!!document.querySelector('#computer')", 10000);
  await js(`window.__wado.sessionsWatch = () => {}; window.__wado.sessionsGet = () => {};`);
  await fx({ type: "phase", stage: 2 }, { type: "hostConfig", state: { grants: { name: "Pixel", files: "none", shells: true, settings: false, host: true, input: true, clipboard: true, join: true, takeover: true } } });
}

await open();
await fx({ type: "relayCaps", caps }, { type: "sessions", sessions, here: "123456789:9" },
  { type: "linkTest", phase: "done", intent: "join", verdict: verdict("ok", 48200, []) });
ok(await until("document.querySelectorAll('.srow').length === 2"), "two session rows");
ok(await js(`[...document.querySelectorAll('.srow')[0].querySelectorAll('.devchip')].length === 3 && !!document.querySelector('.srow .devchip.lead .leadmark')`), "who is in it: a chip per device, the lead marked");
ok(await js(`[...document.querySelectorAll('.srow')[0].querySelectorAll('.sacts .btn')].map(b => b.textContent.trim()).join('|') === 'End|Take over|Join'`), "someone else's session: End, Take over, Join");
ok(await js(`!document.querySelectorAll('.srow')[1].querySelector('.sacts .btn.primary') && /full/.test(document.querySelectorAll('.srow')[1].textContent)`), "a full one: no Join, and it says why");
await shot("01-sessions-two-rows", ".sessions2");
await shot("02-link-test-ok", ".lmeter");

await fx({ type: "linkTest", phase: "run", intent: "join" });
await sleep(300);
const r = await cdp("Page.captureScreenshot", { format: "png" });
writeFileSync(join(OUT, "03-link-test-measuring.png"), Buffer.from(r.result.data, "base64"));
ok(await js(`!!document.querySelector('.lmfill.sweep')`), "measuring: the meter sweeps");
await fx({ type: "linkTest", phase: "done", intent: "join", verdict: verdict("warn", 5200, ["this device's link is slower than the stream: everyone's picture drops to 4.4 Mbps while it is in"], 2) });
await shot("04-link-test-warn", ".lmeter");
await fx({ type: "linkTest", phase: "done", intent: "join", verdict: verdict("refuse", 900, ["this device's link carried 0.9 Mbps — a desktop needs at least 1.5 Mbps, so the picture would break up"], 1) });
await shot("05-link-test-refused", ".lmeter");
ok(await js(`/Would not work/.test(document.querySelector('.lmverdict').textContent)`), "refused: said plainly");

// Take over: the confirm names who will be disconnected.
await js(`(() => { const b = [...document.querySelectorAll('.srow')[0].querySelectorAll('.sacts .btn')].find(b => /Take over/.test(b.textContent)); b.click(); return true; })()`);
await sleep(200);
ok(await js(`/Pixel 8 · Chrome, Laptop · Firefox and 1 more will be disconnected/.test(document.querySelector('.sconfirm').textContent)`), "take over: the confirm names who is dropped");
await shot("06-take-over-confirm", ".sconfirm");

// The join toast.
await fx({ type: "multiNote", text: "Laptop · Firefox joined — they see and control this desktop too" });
await shot("07-join-toast");

// An older relay and an older daemon: no Join, and why.
await open();
await fx({ type: "relayCaps", caps: caps.slice(0, 8) }, { type: "sessions", sessions: [S(1, 1080, 2400, ["foot"], [P(3, "Pixel 8 · Chrome", 3, true)], 4), S(2, 1920, 1080, [], [], 0, { detached: true })], here: "123456789:9" });
ok(await until(`/relay is older than joining/.test(document.querySelector('.sessions2').textContent)`), "an older relay: Join is not offered, and it says why");
ok(await js(`[...document.querySelectorAll('.srow')[1].querySelectorAll('.sacts .btn')].map(b => b.textContent.trim()).join('|') === 'End|Rejoin'`), "nobody in it: a plain Rejoin");
await shot("08-old-relay", ".sessions2");

// Over the picture: other devices' pointers, and who is here.
await fx({ type: "sessionOn" });
await sleep(300);
await js(`window.__wado.fixturePresence([{id:7,name:"Laptop · Firefox",hue:5,x:.32,y:.38,kind:"pointer"},{id:9,name:"iPad · Safari",hue:1,x:.62,y:.66,kind:"touch"}],
  [{id:3,name:"Pixel 8 · Chrome",hue:3,lead:true},{id:7,name:"Laptop · Firefox",hue:5},{id:9,name:"iPad · Safari",hue:1}], 3)`);
ok(await until(`document.querySelectorAll('#wado-presence .ghost.in').length === 2 && document.querySelector('#wado-here.in')`), "two labelled dots and the participants chip");
await shot("09-presence-dots");

cleanup();
console.log(failures ? `\n${failures} failed` : `\nall passed — screenshots in ${OUT}`);
process.exit(failures ? 1 : 0);
