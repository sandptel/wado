// The session never traps (Decision Log 2026-10-30): a session that never gets a picture must
// let the person out, and nothing in flight may put it back on screen once they have left.
// In a real browser, with no computer behind the page (`?fixture`): the session's retries are
// played through the bridge's own relay handlers (`session_started`, as the ICE retry loop's
// rejoins deliver it), not through the fixture's shortcut.
//
//   dx build -p wado-client --platform web
//   node scripts/trap-check.mjs            (needs Chrome; run with the sandbox off)
//   SECS=60 node scripts/trap-check.mjs    (how long the retries go on after leaving)
//
// Checks (390×844):
//   • a never-connecting session shows its state in the picture's place, with Leave, and no
//     floating link card and no "No window open"
//   • Back (the browser's / the WebView's history) leaves it for the computer's page
//   • Files and a settings page then open, and over SECS seconds of retries the session never
//     comes back on screen
//   • the Android app's Back and its own home's doors (`nativeBack`, `nativeGo`) leave it too
//   • a reload (the app's process died in the background) comes back to the computer's page
//   • full screen with no frame: the picture box fills the display, the controls are on screen

import { spawn } from "node:child_process";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const ROOT = new URL("..", import.meta.url).pathname;
const PUB = process.env.WADO_PUB || join(ROOT, "target/dx/wado-client/debug/web/public");
const SECS = +(process.env.SECS || 60);
const T = mkdtempSync(join(tmpdir(), "wado-trap-"));
const HTTP = 8779, CDP = 9379;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const procs = [];
let failures = 0;
const ok = (c, name, d = "") => { if (!c) failures++; console.log(`${c ? "  ok  " : "FAIL  "}${name}${c || !d ? "" : "\n        " + d}`); };
const start = (bin, args) => { const p = spawn(bin, args, { stdio: "ignore" }); procs.push(p); return p; };
const cleanup = () => { for (const p of procs) try { p.kill("SIGTERM"); } catch {} };
for (const sig of ["SIGTERM", "SIGINT"]) process.on(sig, () => { cleanup(); process.exit(1); });

start("python3", ["-m", "http.server", String(HTTP), "--bind", "127.0.0.1", "-d", PUB]);
start("google-chrome-stable", ["--headless=new", "--remote-debugging-port=" + CDP, "--user-data-dir=" + join(T, "chrome"),
  "--no-first-run", "--hide-scrollbars", "--autoplay-policy=no-user-gesture-required", "about:blank"]);
let list;
for (let i = 0; i < 60 && !list; i++) { await sleep(200); try { list = await (await fetch(`http://127.0.0.1:${CDP}/json/list`)).json(); } catch {} }
const ws = new WebSocket(list.find((t) => t.type === "page").webSocketDebuggerUrl);
await new Promise((r) => (ws.onopen = r));
let id = 0; const pend = new Map();
ws.onmessage = (e) => { const m = JSON.parse(e.data); if (m.id && pend.has(m.id)) { pend.get(m.id)(m); pend.delete(m.id); } };
const cdp = (method, params = {}) => new Promise((r) => { const i = ++id; pend.set(i, r); ws.send(JSON.stringify({ id: i, method, params })); });
const js = async (expr) => {
  const r = await cdp("Runtime.evaluate", { expression: expr, returnByValue: true, awaitPromise: true });
  if (r.result?.exceptionDetails) throw new Error(r.result.exceptionDetails.exception?.description || "evaluate failed");
  return r.result?.result?.value;
};
const until = async (expr, ms = 8000) => { const t = Date.now(); while (Date.now() - t < ms) { if (await js(expr).catch(() => false)) return true; await sleep(120); } return false; };
const finish = () => js(`document.getAnimations().forEach(a => { try { a.finish(); } catch {} }); true`);

// A real tap: touch down and up at the element's centre.
async function tap(sel) {
  const r = await js(`(() => { const el = document.querySelector(${JSON.stringify(sel)}); if (!el) return null;
    const b = el.getBoundingClientRect(); return b.width ? { x: b.left + b.width / 2, y: b.top + b.height / 2 } : null; })()`);
  if (!r) return false;
  await cdp("Input.dispatchTouchEvent", { type: "touchStart", touchPoints: [{ x: r.x, y: r.y, id: 1 }] });
  await sleep(60);
  await cdp("Input.dispatchTouchEvent", { type: "touchEnd", touchPoints: [] });
  await sleep(250);
  return true;
}

await cdp("Runtime.enable");
await cdp("Page.enable");
const BASE = `http://127.0.0.1:${HTTP}/`;
const RELAY = "http://10.255.255.1:9", RID = "123456789", KEY = `${RELAY}|${RID}`;
const SETTINGS = JSON.stringify({ conn_mode: "relay", relay_url: RELAY, remote_id: RID, profile: 0, theme: "default-dark",
  profiles: [{ name: "bushido", host: "", conn_mode: "relay", relay_url: RELAY, remote_id: RID }] });

await cdp("Emulation.setDeviceMetricsOverride", { width: 390, height: 844, deviceScaleFactor: 2, mobile: true });
await cdp("Emulation.setTouchEmulationEnabled", { enabled: true, maxTouchPoints: 5 });
await cdp("Page.navigate", { url: BASE });
await sleep(400);
await js(`localStorage.clear(); localStorage.setItem("wado.settings", ${JSON.stringify(SETTINGS)}); true`);
await cdp("Page.navigate", { url: BASE + "?fixture" });
await until("!!window.__wado && !!window.__wado.fixture && !!document.querySelector('#landing .ccard[data-key]')", 12000);
await sleep(300);

const shown = `document.documentElement.dataset.arr !== undefined && !!document.getElementById("surround")`;
const onComputer = `!!document.getElementById("computer") && !document.getElementById("computer").classList.contains("gone")`;
// What the ICE retry loop and a link blip deliver: the daemon's `session_started`.
const RETRY = `(() => { const W = window.__wado; W._relayNegotiate = async () => {}; W._relayHandlers.session_started({ info: {} }); return true; })()`;
// The person picks the desktop: the computer's page, then a session that never gets a picture.
async function enter() {
  if (!(await js(onComputer))) {
    await tap("#landing .ccard[data-key] .cpick");
    await until(onComputer, 4000);
  }
  await js(`window.__wado._left = false; window.__wado.fixture({ type: "link", state: "connecting", path: "", net: "cellular", nat: "", turn: false, why: "", cause: "" }); true`);
  await js(RETRY);
  return until(shown, 4000);
}

ok(await enter(), "a picked session opens, with no picture");
await sleep(600); await finish();
const st = await js(`(() => { const s = document.querySelector(".sstate"); const c = s && s.querySelector(".sstate-card");
  const pic = window.__wado.stageFit.slot("pic"); const b = c && c.getBoundingClientRect();
  return { has: !!c, leave: !!(s && [...s.querySelectorAll("button")].some((x) => /Leave/.test(x.textContent))),
    inside: !!(b && pic && b.left >= pic.x - 1 && b.top >= pic.y - 1 && b.right <= pic.x + pic.w + 1 && b.bottom <= pic.y + pic.h + 1),
    card: !!document.querySelector(".linkcard"), nowin: /No window open/.test(document.body.innerText),
    lane: !!document.querySelector(".lanehint[data-spot='picture']") }; })()`);
ok(st.has && st.leave, "its state is said on the stage, with Leave", JSON.stringify(st));
ok(st.inside, "…inside the picture's slot, not over the bezel or the deck", JSON.stringify(st));
await tap(".linkbadge .linkpill");
ok(!(await js(`!!document.querySelector(".linkcard")`)) && !st.nowin, "no floating link card, no \"No window open\" before a picture");

// Relayed, no picture: folded into the stage, never a card over the deck. (After the page's own
// link tick has said its piece about the unreachable test relay: it speaks only on a change.)
await sleep(2200);
await js(`window.__wado.fixture({ type: "link", state: "relayed", path: "", net: "cellular", nat: "", turn: false, why: "", cause: "" }); true`);
await sleep(400);
const rel = await js(`({ lane: !!document.querySelector(".lanehint[data-spot='picture']"), said: (document.querySelector(".sstate") || {}).textContent || "" })`);
ok(!rel.lane && /no direct path/i.test(rel.said), "relayed with no picture: said on the stage, no lane card over the deck", JSON.stringify(rel));

// Back: the history entry the session pushed.
await js(`history.back(); true`);
ok(await until(`!(${shown})`, 3000) && (await until(onComputer, 3000)), "Back leaves the session for the computer's page");
ok(await js(`window.__wado._left === true && window.__wado.sessionOn === false`), "…and this device stopped its retries (left, not ended)");

// Elsewhere for SECS seconds while the retries go on.
const t0 = Date.now();
let back = 0, opened = { files: false, settings: false };
while (Date.now() - t0 < SECS * 1000) {
  await js(RETRY);
  await sleep(300);
  if (await js(shown)) back++;
  const el = (Date.now() - t0) / 1000;
  if (!opened.files && el > 2) {
    await tap("#computer .acts .act:nth-of-type(2)");
    opened.files = await until("!!document.getElementById('wado-files')", 3000);
  }
  if (opened.files && !opened.settings && el > 6) {
    await js(`window.__wado.files.closeUi(); true`);
    await sleep(600);
    await js(`document.querySelector("#computer .settings .navrow").scrollIntoView({ block: "center" }); true`);
    await sleep(300);
    await tap("#computer .settings .navrow");
    opened.settings = await until("!!document.getElementById('sub')", 3000);
  }
  await sleep(2700);
}
ok(opened.files, "Files opened from the computer's page");
ok(opened.settings, "a settings page opened", await js(`(() => { const r = document.querySelector("#computer .settings .navrow"); if (!r) return "no row";
  const b = r.getBoundingClientRect(); const top = document.elementFromPoint(b.left + b.width / 2, b.top + b.height / 2);
  return JSON.stringify({ b: [b.top, b.height], top: top && (top.id || top.className), files: !!document.getElementById("wado-files") }); })()`));
ok(back === 0, `over ${SECS} s of retries the session never came back on screen`, `back ${back}×`);

// The Android app: its Back, and a door on its own home, both leave a session.
await tap("#sub .phead button"); await until("!document.getElementById('sub')", 3000);
ok(await enter(), "picked again");
await js(`window.__wado.fixture({ type: "nativeBack" }); true`);
ok(await until(`!(${shown})`, 3000) && (await until(onComputer, 3000)), "the app's Back leaves it for the computer's page");
ok(await enter(), "picked again");
await js(`window.__wado.fixture({ type: "nativeGo", key: ${JSON.stringify(KEY)}, go: "files" }); true`);
ok(await until(`!(${shown})`, 3000) && (await until("!!document.getElementById('wado-files')", 4000)),
  "a door on the app's own home leaves the session and opens there");
await js(RETRY); await sleep(800);
ok(!(await js(shown)), "…and a retry after that does not bring it back");

// Full screen with no frame.
await js(`window.__wado.files.closeUi(); true`); await sleep(400);
ok(await enter(), "picked again");
await js(`document.querySelector("[data-arrange='toggle']").click(); true`);
await until("document.documentElement.dataset.arr === 'cockpit'", 3000);
await sleep(900); await finish();
const fs = await js(`(() => { const v = document.getElementById("wado-video").getBoundingClientRect();
  const off = [...document.querySelectorAll(".rail .barbtn, #wado-bar .barbtn, .sstate-card")]
    .filter((el) => !el.closest("[inert]") && getComputedStyle(el).visibility !== "hidden" && el.getBoundingClientRect().width > 0)
    .filter((el) => { const b = el.getBoundingClientRect(); return b.left < -1 || b.top < -1 || b.right > innerWidth + 1 || b.bottom > innerHeight + 1; })
    .map((el) => el.getAttribute("aria-label") || el.className);
  const s = document.querySelector(".sstate-card"); const sb = s && s.getBoundingClientRect();
  return { v: [v.left, v.top, v.width, v.height].map(Math.round), off, state: !!s,
    centred: !!sb && Math.abs(sb.left + sb.width / 2 - innerWidth / 2) < 2 }; })()`);
ok(fs.v[0] === 0 && fs.v[1] === 0 && fs.v[2] === 390 && fs.v[3] === 844, "full screen, no frame: the picture box fills the display", JSON.stringify(fs.v));
ok(!fs.off.length && fs.state && fs.centred, "full screen, no frame: every control on screen, the state centred", JSON.stringify(fs));

// The app's process died in the background: it comes back to the computer's page, not the session.
await js(`window.__wado.fixture({ type: "nativeBack" }); true`); await sleep(600);
ok(await until(onComputer, 3000), "Back from full screen with no frame reaches the computer's page");
await cdp("Page.reload");
await until("!!window.__wado && !!window.__wado.fixture", 10000);
ok(await until(onComputer, 5000) && !(await js(shown)), "a reload comes back to the computer's page, not the session");

cleanup();
console.log(failures ? `\n${failures} failed` : "\nall passed");
process.exit(failures ? 1 : 0);
