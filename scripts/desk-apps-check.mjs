// A Mac's own apps on its computer page, in a real browser with no computer behind it
// (`?fixture`): the daemon's side is played by hand — a Mac config with `desk_apps`, then
// `apps_list` frames into relay.js's own handler — and what the page sends is caught.
//
//   dx build -p wado-client --platform web
//   node scripts/desk-apps-check.mjs [out-dir]      (needs Chrome; CHROME=<path> to pick one)
//
// Checks at 390×844 (touch): the dock is the Mac's Dock order, a divider, then the running extras;
// front / hidden / running on each icon; it asks for the list while shown; a tap sends the right
// `app_do` and bounces until the app runs; a long press and a right-click open the menu with the
// items for each state, and Force Quit asks first; All apps opens Apps (Open now order, A–Z,
// search, an empty search says so). Screenshots go to out-dir.

import { spawn } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const ROOT = new URL("..", import.meta.url).pathname;
const PUB = process.env.WADO_PUB || join(ROOT, "target/dx/wado-client/debug/web/public");
const OUT = process.argv[2] || join(tmpdir(), "wado-desk-apps-shots");
mkdirSync(OUT, { recursive: true });
const T = mkdtempSync(join(tmpdir(), "wado-desk-"));
const HTTP = 8774, CDP = 9374;
const MAC_CHROME = "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";
const CHROME = process.env.CHROME || (existsSync(MAC_CHROME) ? MAC_CHROME : "google-chrome-stable");
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const procs = [];
let failures = 0;
const ok = (c, name, d = "") => { if (!c) failures++; console.log(`${c ? "  ok  " : "FAIL  "}${name}${c || !d ? "" : "\n        " + d}`); };
const start = (bin, args) => { const p = spawn(bin, args, { stdio: "ignore" }); procs.push(p); return p; };
const cleanup = () => { for (const p of procs) try { p.kill("SIGTERM"); } catch {} };
for (const sig of ["SIGTERM", "SIGINT"]) process.on(sig, () => { cleanup(); process.exit(1); });

start("python3", ["-m", "http.server", String(HTTP), "--bind", "127.0.0.1", "-d", PUB]);
start(CHROME, ["--headless=new", "--remote-debugging-port=" + CDP, "--user-data-dir=" + join(T, "chrome"),
  "--no-first-run", "--hide-scrollbars", "about:blank"]);
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
const shot = async (name) => {
  await sleep(400);
  await js(`document.getAnimations().forEach(a => { try { a.finish(); } catch {} }); true`);
  await sleep(100);
  const r = await cdp("Page.captureScreenshot", { format: "png" });
  writeFileSync(join(OUT, name + ".png"), Buffer.from(r.result.data, "base64"));
};

await cdp("Runtime.enable");
await cdp("Page.enable");
const BASE = `http://127.0.0.1:${HTTP}/`;
const RELAY = "http://10.255.255.1:9", RID = "123456789";

const A = (name, o = {}) => ({ name, exec: `/Applications/${name}.app`, running: false, ...o });
const APPS = [
  A("Safari", { running: true, dock: 1 }),
  A("Finder", { running: true, dock: 0 }),
  A("Notes", { dock: 2 }),
  A("Zed", { running: true, front: true }),
  A("Calculator", { running: true, away: true }),
  A("Chess"),
];
const feed = (apps) => js(`window.__wado._relayHandlers.apps_list({ type: "apps_list", apps: ${JSON.stringify(apps)} }); true`);
const sent = () => js(`window.__sent.slice()`);
const dock = () => js(`[...document.querySelectorAll(".dapps .dshelf > *")].map(e => e.classList.contains("dsep") ? "|" : e.dataset.dapp.replace(/^.*\\/|\\.app$/g, ""))`);
const tile = (name) => `document.querySelector('[data-dapp="/Applications/${name}.app"]')`;
const menuRows = () => js(`[...document.querySelectorAll(".usheet .gpickrow")].map(b => b.textContent.trim())`);
const closeMenu = () => js(`document.querySelector(".uscrim").click(); true`);
const press = (sel) => js(`(() => { const b = ${sel}; const r = b.getBoundingClientRect(); const o = { bubbles: true, pointerType: "touch", clientX: r.x + 20, clientY: r.y + 20 };
  b.dispatchEvent(new PointerEvent("pointerdown", o)); return new Promise(res => setTimeout(() => { b.dispatchEvent(new PointerEvent("pointerup", o)); b.click(); res(true); }, 520)); })()`);

await cdp("Emulation.setDeviceMetricsOverride", { width: 390, height: 844, deviceScaleFactor: 2, mobile: true });
await cdp("Emulation.setTouchEmulationEnabled", { enabled: true, maxTouchPoints: 5 });
const settings = JSON.stringify({ conn_mode: "relay", relay_url: RELAY, remote_id: RID, profile: 0, tips_seen: ["tour"],
  profiles: [{ name: "MacBook", host: "", conn_mode: "relay", relay_url: RELAY, remote_id: RID }] });
await cdp("Page.navigate", { url: BASE });
await sleep(500);
await js(`localStorage.clear(); localStorage.setItem("wado.settings", ${JSON.stringify(settings)}); true`);
await cdp("Page.navigate", { url: BASE + "?fixture" });
await until("!!window.__wado && !!window.__wado.fixture", 10000);
// The relay link, played: up, and every frame sent kept.
await js(`(() => { const W = window.__wado; window.__sent = [];
  Object.defineProperty(W, "relayUp", { get: () => true, set() {}, configurable: true });
  W.relaySendMsg = (m) => { window.__sent.push(m); return true; };
  W.fixture({ type: "hostConfig", state: { platform: "macos", host_caps: { on: ["desk_apps"], off: [
    { cap: "session", why: "A screen session is coming to macOS.", later: true }] } } });
  W.fixture({ type: "hostState", state: {} });
  W.fixture({ type: "nativeGo", key: ${JSON.stringify(RELAY + "|" + RID)}, go: "" });
  return true; })()`);
ok(await until(`!!document.querySelector("#computer .dapps")`), "a Mac with desk_apps: the Apps card is on its page");
ok(await js(`!document.querySelector("#computer .snew") && !/Nothing running/.test(document.querySelector("#computer").textContent)`),
  "…and no Start a new session, no empty session list");
ok(await js(`!!document.querySelector("#computer .dapps .lmeter")`), "…and the link test stays");
ok(await until(`window.__sent.some(m => m.type === "apps_request" && m.lean)`, 2000), "it asks for the list (lean) while shown");

await feed(APPS);
// Icons for three, drawn here; the rest stay letters.
await js(`(() => { const icon = (hue) => { const c = document.createElement("canvas"); c.width = c.height = 96; const g = c.getContext("2d");
    g.fillStyle = "hsl(" + hue + " 70% 55%)"; g.beginPath(); g.roundRect(6, 6, 84, 84, 20); g.fill(); return c.toDataURL(); };
  window.__wado._relayHandlers.apps_icons_list({ type: "apps_icons_list", icons: {
    "/Applications/Finder.app": icon(210), "/Applications/Safari.app": icon(190), "/Applications/Zed.app": icon(30) } }); return true; })()`);
ok(await until(`document.querySelectorAll(".dapps .dtile img").length === 3`), "icons arrive into the dock; letters stand in for the rest");
ok(await until(`document.querySelectorAll(".dapps .dtile").length === 5`), "five icons: the Dock's three and two running extras");
const order = await dock();
ok(JSON.stringify(order) === JSON.stringify(["Finder", "Safari", "Notes", "|", "Calculator", "Zed"]), "the Mac's Dock order, a divider, then the running extras A–Z", JSON.stringify(order));
const cls = await js(`Object.fromEntries([...document.querySelectorAll(".dapps .dtile")].map(e => [e.dataset.dapp.replace(/^.*\\/|\\.app$/g, ""), e.className]))`);
ok(/\bfront\b/.test(cls.Zed) && /\brun\b/.test(cls.Zed), "Zed is in front", cls.Zed);
ok(/\baway\b/.test(cls.Calculator), "Calculator is hidden (dimmed)", cls.Calculator);
ok(/\brun\b/.test(cls.Safari) && !/front|away/.test(cls.Safari), "Safari runs", cls.Safari);
ok(!/\brun\b/.test(cls.Notes), "Notes is not open", cls.Notes);
const dim = await js(`getComputedStyle(${tile("Calculator")}.querySelector(".dicon")).opacity`);
ok(Math.abs(+dim - 0.55) < 0.05, "a hidden app's icon is at ~55%", dim);
const asked = (await sent()).filter((m) => m.type === "apps_icons");
ok(asked.length > 0 && asked[0].execs.length <= 24 && asked[0].execs[0].endsWith("Finder.app"), "icons are asked for, the Dock's first, ≤24", JSON.stringify(asked[0]));
await shot("dock");

// A tap: Notes is not open → open; it bounces until it runs.
await js(`window.__sent.length = 0; ${tile("Notes")}.click(); true`);
ok((await sent()).some((m) => m.type === "app_do" && m.action === "open" && m.exec.endsWith("Notes.app")), "a tap on Notes sends open");
ok(await until(`${tile("Notes")}.classList.contains("launching")`, 2000), "…and it bounces");
await feed(APPS.map((a) => (a.name === "Notes" ? { ...a, running: true } : a)));
ok(await until(`!${tile("Notes")}.classList.contains("launching") && ${tile("Notes")}.classList.contains("run")`, 2000), "…until the list says it runs");
await feed(APPS);
await js(`window.__sent.length = 0; ${tile("Calculator")}.click(); true`);
ok((await sent()).some((m) => m.type === "app_do" && m.action === "unhide"), "a tap on a hidden app sends unhide");

// The menu: long press (touch) and right-click.
await js(`window.__sent.length = 0; true`);
await press(tile("Safari"));
ok(await until(`!!document.querySelector(".usheet")`, 2000), "a long press opens the menu");
ok(!(await sent()).some((m) => m.type === "app_do"), "…and the tap that ends it does not also open the app");
ok(await js(`/Safari/.test(document.querySelector(".uhead b").textContent) && /Running/.test(document.querySelector(".uhead span").textContent)`), "…headed with its name and Running");
ok(JSON.stringify(await menuRows()) === JSON.stringify(["Show", "Hide", "Quit", "Force Quit"]), "running: Show, Hide, Quit, Force Quit", JSON.stringify(await menuRows()));
await shot("menu-running");
await js(`[...document.querySelectorAll(".usheet .gpickrow")].find(b => b.textContent.trim() === "Hide").click(); true`);
ok((await sent()).some((m) => m.type === "app_do" && m.action === "hide" && m.exec.endsWith("Safari.app")), "Hide sends hide");
ok(await until(`!document.querySelector(".usheet")`, 2000), "…and closes the menu");

await js(`${tile("Calculator")}.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true })); true`);
ok(await until(`!!document.querySelector(".usheet")`, 2000), "a right-click opens the menu");
ok(JSON.stringify(await menuRows()) === JSON.stringify(["Show", "Quit", "Force Quit"]) && /Hidden/.test(await js(`document.querySelector(".uhead span").textContent`)),
  "hidden: Show, Quit, Force Quit", JSON.stringify(await menuRows()));
await closeMenu();
await js(`${tile("Notes")}.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true })); true`);
ok(await until(`!!document.querySelector(".usheet")`, 2000) && JSON.stringify(await menuRows()) === JSON.stringify(["Open"]), "not open: Open", JSON.stringify(await menuRows()));
await closeMenu();

await js(`window.__sent.length = 0; ${tile("Zed")}.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true })); true`);
await until(`!!document.querySelector(".usheet")`, 2000);
ok(/In front/.test(await js(`document.querySelector(".uhead span").textContent`)), "the front app's menu says In front");
await js(`[...document.querySelectorAll(".usheet .gpickrow")].find(b => b.textContent.trim() === "Force Quit").click(); true`);
ok(await until(`/Unsaved changes in Zed will be lost/.test(document.querySelector(".usheet").textContent)`, 2000), "Force Quit asks first");
ok(!(await sent()).some((m) => m.type === "app_do"), "…and sends nothing yet");
await shot("menu-force-quit");
await js(`document.querySelector(".usheet .btn.danger").click(); true`);
ok((await sent()).some((m) => m.type === "app_do" && m.action === "force_quit" && m.exec.endsWith("Zed.app")), "confirmed, it sends force_quit");

// All apps.
await js(`[...document.querySelectorAll(".dapps .sect button")].find(b => /All apps/.test(b.textContent)).click(); true`);
ok(await until(`!!document.querySelector("#sub .dgrid")`, 3000), "All apps opens the Apps page");
const grids = await js(`[...document.querySelectorAll("#sub .dgrid")].map(g => [...g.querySelectorAll(".dtile")].map(e => e.querySelector(".dname").textContent))`);
ok(JSON.stringify(grids[0]) === JSON.stringify(["Zed", "Finder", "Safari", "Calculator"]), "Open now: the front app, then the Dock's order, then names", JSON.stringify(grids[0]));
ok(JSON.stringify(grids[1]) === JSON.stringify(["Calculator", "Chess", "Finder", "Notes", "Safari", "Zed"]), "All apps: A–Z", JSON.stringify(grids[1]));
ok(await js(`document.activeElement !== document.querySelector("#sub .dsearch")`), "on a phone the search box does not take focus (no keyboard)");
await shot("apps-page");
const type = (v) => js(`(() => { const i = document.querySelector("#sub .dsearch"); i.value = ${JSON.stringify(v)}; i.dispatchEvent(new Event("input", { bubbles: true })); return true; })()`);
await type("saf");
ok(await until(`[...document.querySelectorAll("#sub .dgrid .dname")].map(e => e.textContent).join() === "Safari,Safari"`, 2000), "search narrows both sections");
await type("photoshop");
ok(await until(`/No app is called “photoshop”/.test(document.querySelector("#sub").textContent)`, 2000), "an empty search says so");
await shot("apps-search-empty");
await type("");
await js(`window.__sent.length = 0; true`);
await sleep(3300);
ok((await sent()).some((m) => m.type === "apps_request"), "still asked every 3 s while Apps is up");
await js(`window.__wado.subClose ? window.__wado.subClose() : history.back(); true`);
ok(await until(`!document.querySelector("#sub .dgrid")`, 3000), "Back returns to the computer");

// Not a Mac: as before.
await js(`window.__wado.fixture({ type: "hostConfig", state: { platform: "linux", host_caps: { on: ["desk_apps"], off: [] } } }); true`);
ok(await until(`!document.querySelector("#computer .dapps") && !!document.querySelector("#computer .sessions2")`, 3000), "not a Mac: Sessions as before, no Apps card");

cleanup();
console.log(failures ? `\n${failures} failed` : `\nall passed — screenshots in ${OUT}`);
process.exit(failures ? 1 : 0);
