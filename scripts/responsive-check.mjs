// Does the client fit every screen at every size the person can pick? With no computer behind the
// page (`?fixture`), each view is opened at each device size, then measured at each text size ×
// interface size (DESIGN.md, "Size"):
//
//   • nothing scrolls the page sideways, and no view is wider than the screen;
//   • nothing is clipped: a box that hides its overflow holds all of its content, unless it is an
//     intended one-line truncation (`text-overflow: ellipsis`) whose full text is still reachable
//     (its own or a near ancestor's `title` / `aria-label`);
//   • on touch sizes, every control is at least 44 × 44 px (but the console deck's Left / Right
//     click zones, 30 px tall by design — DESIGN.md, "Pocket Console").
//
//   dx build -p wado-client --platform web
//   node scripts/responsive-check.mjs [out-dir]      (needs Chrome; run with the sandbox off)
//
// WADO_PUB=<dir> checks another build. A build from before the size settings (no `W.uiSize`) is
// measured with the text scaled the way a WebView's textZoom does it (every font size multiplied),
// and at the cozy interface only. ONLY=<"name WxH" substring> narrows the devices; VERBOSE=1 lists
// every failure rather than the first few of each kind. Screenshots of the Computer page go to
// out-dir. VIEW=<view name substring> narrows the views.

import { spawn } from "node:child_process";
import { mkdirSync, mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const ROOT = new URL("..", import.meta.url).pathname;
const PUB = process.env.WADO_PUB || join(ROOT, "target/dx/wado-client/debug/web/public");
const OUT = process.argv[2] || join(tmpdir(), "wado-responsive-shots");
mkdirSync(OUT, { recursive: true });
const T = mkdtempSync(join(tmpdir(), "wado-responsive-"));
const OFF = +(process.env.PORT_OFFSET || 0); // two runs at once need different ports
const HTTP = 8783 + OFF, CDP = 9383 + OFF;
const VERBOSE = !!process.env.VERBOSE;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const procs = [];
const cleanup = () => { for (const p of procs) try { p.kill("SIGTERM"); } catch {} };
for (const sig of ["SIGTERM", "SIGINT"]) process.on(sig, () => { cleanup(); process.exit(1); });
const start = (bin, args) => { const p = spawn(bin, args, { stdio: "ignore" }); procs.push(p); return p; };

const DEVICES = [
  { name: "small phone", w: 360, h: 740, touch: true },
  { name: "phone", w: 390, h: 844, touch: true },
  { name: "phone XL", w: 412, h: 915, touch: true },
  { name: "large phone", w: 430, h: 932, touch: true },
  { name: "landscape phone", w: 844, h: 390, touch: true },
  { name: "tablet", w: 820, h: 1180, touch: true },
  { name: "tablet landscape", w: 1180, h: 820, touch: true },
  { name: "laptop", w: 1440, h: 900, touch: false },
  { name: "desktop", w: 1920, h: 1080, touch: false },
  { name: "desktop tall", w: 2000, h: 1270, touch: false },
].filter((d) => !process.env.ONLY || `${d.name} ${d.w}x${d.h}`.includes(process.env.ONLY));
const TEXTS = [85, 100, 130, 150];
const VIEW_ONLY = process.env.VIEW || "";
const INTERFACES = ["compact", "cozy", "large"];

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
  if (r.result?.exceptionDetails) throw new Error(r.result.exceptionDetails.exception?.description || JSON.stringify(r.result.exceptionDetails).slice(0, 300));
  return r.result?.result?.value;
};
const until = async (expr, ms = 8000) => { const t = Date.now(); while (Date.now() - t < ms) { if (await js(expr).catch(() => false)) return true; await sleep(120); } return false; };
const fx = (...events) => js(`(() => { const W = window.__wado; ${events.map((e) => `W.fixture(${JSON.stringify(e)});`).join("")} return true; })()`);
const settle = async () => { await sleep(80); await js(`document.getAnimations().forEach((a) => { try { a.finish(); } catch (_) {} }); true`); await sleep(60); };

await cdp("Runtime.enable");
await cdp("Page.enable");
const BASE = `http://127.0.0.1:${HTTP}/`;

// ── what the daemon would have said ──────────────────────────────────────────────────────
const KEY = "123456789";
const SETTINGS = JSON.stringify({ conn_mode: "relay", relay_url: "http://10.255.255.1:9", remote_id: KEY, profile: 0,
  profiles: [
    { name: "bushido", host: "", conn_mode: "relay", relay_url: "http://10.255.255.1:9", remote_id: KEY },
    { name: "Living-room media centre with a long name", host: "", conn_mode: "relay", relay_url: "http://10.255.255.1:9", remote_id: "987654321" },
  ] });
const INFO = { user: "bushido", hostname: "bushido", os: "NixOS 26.11 (Zokor)", os_id: "nixos", kernel: "6.17.4", uptime_s: 93000,
  cpu: "AMD Ryzen 7 7840U w/ Radeon 780M Graphics", cores: 16, gpu: "AMD Radeon 780M", mem_used_mb: 9123, mem_total_mb: 31000,
  shell: "fish", desktop: "Hyprland" };
const HOST = { audio: { sinks: [], streams: [], default_sink: "", phone_sink: null }, wifi: null, bluetooth: null, awake: false,
  brightness: 0.6, media: [], info: INFO };
const GRANTS = { name: "Pixel", files: "rw", shells: true, settings: true, host: true, input: true, clipboard: true, join: true, takeover: true, paired: true };
const P = (id, name, hue, lead) => ({ id, name, hue, lead, since_ms: 1e12 + id, link: "" });
const SESSIONS = [{ instance: KEY + ":1", started_ms: 1, age_s: 1300, width: 1080, height: 2400, fps: 60,
  apps: ["org.mozilla.firefox", "foot", "code"], viewer: "Pixel 8 · Chrome", detached: false, lead: "Pixel 8 · Chrome",
  participants: [P(3, "Pixel 8 · Chrome", 3, true), P(7, "Laptop · Firefox with a long device name", 5, false)], max_participants: 4, kbps: 6000 }];
const CLIPS = [
  { id: "1", kind: "Text", preview: "cargo test -p wado-client --features very-long-feature-name-that-does-not-wrap", pinned: true, at_ms: Date.now() - 60000, from: "desktop" },
  { id: "2", kind: "Link", preview: "https://example.org/a/really/long/path/that/keeps/going/without/any/spaces/at/all/index.html", pinned: false, at_ms: Date.now() - 3600000, from: "Laptop" },
  { id: "3", kind: "Text", preview: "Short note", pinned: false, at_ms: 0, from: "" },
];
const caps = ["park", "hold", "takeover", "gate", "ping", "leave", "seat_hold", "occupancy", "control", "room_strict"];
const verdict = { level: "ok", reasons: [], down_kbps: 48200, rtt_ms: 28, suggested_max: 4, max_participants: 4, participants: 2, session_kbps: 6000,
  host: { mem_avail_mb: 9000, load_per_core: 0.3, encoder: "hardware", sessions: 1, max_sessions: 4, uplink_kbps: 48200 } };
const FILES = {
  quick: { roots: [{ name: "Home", path: "/home/bushido" }], places: [{ name: "Downloads", path: "/home/bushido/Downloads" }], recent: [] },
  list: { entries: [
    { name: "a-very-long-file-name-from-a-camera-export-2026-10-30-IMG_20261030_101112_HDR.jpg", kind: "file", size: 4812331, mtime: 1790000000 },
    { name: "Documents", kind: "dir", size: 0, mtime: 1790000000 },
    { name: "notes.md", kind: "file", size: 1200, mtime: 1790000000 },
  ] },
};

// ── views ────────────────────────────────────────────────────────────────────────────────
// Each opens itself from a fresh load and names the element the person is looking at.
async function load(query) {
  await cdp("Page.navigate", { url: BASE + query });
  await until("!!window.__wado && !!window.__wado.fixture", 10000);
}
async function computer() {
  await load("?fixture&go=session");
  await until("!!document.querySelector('#computer')", 10000);
  await js(`window.__wado.sessionsWatch = () => {}; window.__wado.sessionsGet = () => {}; true`);
  await fx({ type: "phase", stage: 2 }, { type: "hostConfig", state: { grants: GRANTS } }, { type: "hostState", state: HOST },
    { type: "relayCaps", caps }, { type: "sessions", sessions: SESSIONS, here: KEY + ":9" },
    { type: "linkTest", phase: "done", intent: "join", verdict },
    { type: "clipHistory", entries: CLIPS, available: true });
  await sleep(900);
}
async function sub(title, fromComputer) {
  if (fromComputer) await computer(); else { await load("?fixture"); await sleep(700); }
  const root = fromComputer ? "#computer" : "#landing";
  const ok = await js(`(() => { const b = [...document.querySelectorAll(${JSON.stringify(root + " .navrow")})]
    .find((x) => x.querySelector("b")?.textContent.trim() === ${JSON.stringify(title)}); if (b) b.click(); return !!b; })()`);
  if (!ok) return false;
  return until("!!document.querySelector('#sub')", 4000);
}
const VIEWS = [
  { name: "home", root: "#landing", open: async () => { await load("?fixture"); await sleep(900); } },
  { name: "pairing", root: "#landing", open: async () => { await load("?fixture"); await fx({ type: "unpaired", reason: "This device is not paired with bushido yet. Type the code it shows, or scan its QR." }); await sleep(500); } },
  { name: "computer", root: "#computer", open: computer, shoot: true },
  ...["Appearance", "Input feel", "Gamepad", "Quick tiles", "Diagnostics"].map((t) => ({ name: "settings: " + t, root: "#sub", open: () => sub(t, false), shoot: t === "Appearance" })),
  ...["Display & session", "Sound & devices", "Connection & trust"].map((t) => ({ name: "settings: " + t, root: "#sub", open: () => sub(t, true) })),
  { name: "passkey sheet", root: ".pksheet, [aria-labelledby=pktitle]", open: async () => {
    await computer();
    await fx({ type: "passkey", state: { registered: true, unlocked: false, idle_left_s: 0, idle_s: 900, error: "", origin: "" }, need: "files", supported: true, platform: true });
    return until("!!document.getElementById('pktitle')", 4000);
  } },
  { name: "files", root: "#wado-files", open: async () => {
    await computer();
    await js(`(() => { const F = window.__wado.files; F.connect = async () => {};
      F.req = async (op) => (${JSON.stringify(FILES)})[op] || {}; window.__wado.filesOpen(); return true; })()`);
    await sleep(500);
    await js(`(() => { const row = [...document.querySelectorAll("#wado-files button, #wado-files [role=button]")].find((b) => /Home/.test(b.textContent)); if (row) row.click(); return true; })()`);
    await sleep(500);
  } },
  { name: "trackpad", root: "#wado-trackpad", open: async () => {
    await computer();
    // Unlocked, as after the passkey: the trackpad is behind it.
    await js(`window.__wado.passkey.state.unlocked = true; window.__wado.padOpen(); true`);
    return until("!!document.getElementById('wado-trackpad') && !document.getElementById('wado-trackpad').hidden", 4000);
  } },
  // Virtual input (docs/superpowers/specs/2026-10-30-virtual-input-design.md): the hub, its three
  // pages, and the gamepad screen — playing and in its editor.
  ...[["hub", null], ["touchpad", "Touchpad"], ["keyboard", "Keyboard"], ["gamepad", "Gamepad"]].map(([v, page]) => ({
    name: "virtual input: " + v, root: "#sub", open: async () => {
      await computer();
      await fx({ type: "hostConfig", state: { grants: GRANTS, caps: ["pad-gamepad", "pad-hypr"] } });
      await sleep(300);
      const ok = await js(`(() => { const b = document.querySelector("#computer .vihead"); if (b) b.click(); return !!b; })()`);
      if (!ok || !(await until("!!document.querySelector('#sub .vihub')", 4000))) return false;
      if (!page) return true;
      await sleep(600);
      await js(`document.querySelector(${JSON.stringify(`[aria-label="${page} settings"]`)}).click(); true`);
      return until("!!document.querySelector('#sub .vipage')", 4000);
    },
  })),
  ...["play", "edit"].map((state) => ({
    name: "gamepad: " + state, root: "#wado-gpad", open: async () => {
      await computer();
      await fx({ type: "hostConfig", state: { grants: GRANTS, caps: ["pad-gamepad"] } });
      await js(`(() => { const W = window.__wado; W.pad.start = () => {}; W.pad.stop = () => {}; W.pad.send = () => true;
        if (W.passkey) W.passkey.state.unlocked = true; W.gpadOpen({ where: "away", state: ${JSON.stringify(state)}, name: "bushido" }); return true; })()`);
      if (state === "edit") await js(`window.__wado.gpadEdit.select("a"); true`);
      return until("!!document.getElementById('wado-gpad') && !document.getElementById('wado-gpad').hidden", 4000);
    },
  })),
  { name: "console", root: "#stage", open: async () => {
    await load("?fixture");
    await fx({ type: "sessionOn" }, { type: "windows", workspace: 1, windows: [
      { id: 1, title: "Repotting a monstera — a long window title that has to give way", app_id: "firefox", focused: true, workspace: 1 },
      { id: 2, title: "~/src/wado — cargo test", app_id: "foot", focused: false, workspace: 1 }] },
      { type: "link", state: "direct", path: "lan", rtt_ms: 23, net: "wifi" });
    await sleep(900);
  } },
];

// ── the size, as the person would pick it ────────────────────────────────────────────────
// After: the client's own setting. Before (no `W.uiSize`): every font size multiplied, which is
// what the Android WebView's textZoom did to it.
const HAS_SIZE = `!!(window.__wado && window.__wado.uiSize)`;
const applySize = (text, iface) => js(`(() => {
  const W = window.__wado;
  if (!W) return false;
  if (W.uiSize) { W.uiSize.set("follow", false); W.uiSize.set("text", ${text}); W.uiSize.set("interface", ${JSON.stringify(iface)}); return true; }
  const els = [...document.querySelectorAll("body *")];
  for (const e of els) if (e.dataset.rcFont !== undefined) { e.style.fontSize = e.dataset.rcFont; delete e.dataset.rcFont; }
  if (${text} === 100) return true;
  const sizes = els.map((e) => parseFloat(getComputedStyle(e).fontSize));
  els.forEach((e, i) => { e.dataset.rcFont = e.style.fontSize; e.style.fontSize = (sizes[i] * ${text} / 100) + "px"; });
  return true;
})()`);

// ── the measure ──────────────────────────────────────────────────────────────────────────
const MEASURE = (rootSel, touch) => `(() => {
  const out = [];
  const vw = innerWidth;
  const doc = document.scrollingElement;
  if (doc.scrollWidth > vw + 1) out.push({ kind: "page scrolls sideways", what: "document", d: doc.scrollWidth - vw });
  const root = document.querySelector(${JSON.stringify(rootSel)});
  if (!root) return { missing: true, out };
  const shown = (e) => e.checkVisibility({ opacityProperty: true, visibilityProperty: true }) && !e.closest("[inert], [aria-hidden=true], [hidden]");
  const name = (e) => {
    let s = e.tagName.toLowerCase();
    if (e.id) s += "#" + e.id;
    if (typeof e.className === "string" && e.className.trim()) s += "." + e.className.trim().split(/\\s+/).slice(0, 2).join(".");
    const t = (e.getAttribute("aria-label") || e.textContent || "").trim().replace(/\\s+/g, " ").slice(0, 32);
    return t ? s + " \\"" + t + "\\"" : s;
  };
  const reachable = (e) => {
    const full = (e.textContent || "").trim();
    for (let a = e, i = 0; a && i < 4; a = a.parentElement, i++) {
      const t = (a.getAttribute("title") || "") + " " + (a.getAttribute("aria-label") || "");
      if (full && t.includes(full)) return true;
    }
    return false;
  };
  const rr = root.getBoundingClientRect();
  if (root.scrollWidth > root.clientWidth + 1 && getComputedStyle(root).overflowX !== "auto" && getComputedStyle(root).overflowX !== "scroll")
    out.push({ kind: "view wider than the screen", what: name(root), d: root.scrollWidth - root.clientWidth });
  for (const e of root.querySelectorAll("*")) {
    if (!shown(e)) continue;
    const cs = getComputedStyle(e);
    const r = e.getBoundingClientRect();
    if (!r.width || !r.height) continue;
    // Intended scrollers are left alone; so is the stage's picture, whose size is the session's.
    const scrollerX = e.closest("[data-scroll-x]") || [...(function* () { for (let a = e.parentElement; a && a !== root.parentElement; a = a.parentElement) yield a; })()]
      .some((a) => ["auto", "scroll"].includes(getComputedStyle(a).overflowX));
    if (e.matches("video, canvas, #wado-video, #wado-video *")) continue;
    if (!scrollerX && (r.right > vw + 1 || r.left < -1) && cs.position !== "fixed")
      out.push({ kind: "off the screen", what: name(e), d: Math.round(Math.max(r.right - vw, -r.left)) });
    if (${touch} && e.matches("button, a[href], select, input:not([type=hidden]):not([type=range]), textarea, [role=button], [role=switch], [role=radio], [role=tab], [role=checkbox], summary")
      && !e.disabled && cs.pointerEvents !== "none" && !e.matches(".dpbtn") && (r.width < 43.5 || r.height < 43.5))
      out.push({ kind: "touch target under 44px", what: name(e), d: Math.round(Math.min(r.width, r.height)) });
  }
  // Text that a box cuts off: each text node's own extent against every box above it that hides
  // its overflow. Cut by the one-line box that holds it with an ellipsis: intended, if the whole
  // text can still be reached. Cut any other way: clipped.
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
  const range = document.createRange();
  const seen = new Set();
  for (let t = walker.nextNode(); t; t = walker.nextNode()) {
    if (!t.data.trim()) continue;
    const p = t.parentElement;
    if (!p || !shown(p) || p.closest("svg, .sr, [aria-hidden=true]")) continue;
    range.selectNodeContents(t);
    const tr = range.getBoundingClientRect();
    if (!tr.width || !tr.height) continue;
    for (let a = p; a && a !== root.parentElement; a = a.parentElement) {
      const cs = getComputedStyle(a);
      const hx = ["hidden", "clip"].includes(cs.overflowX), hy = ["hidden", "clip"].includes(cs.overflowY);
      if (!hx && !hy) continue;
      const ar = a.getBoundingClientRect();
      if (["auto", "scroll"].includes(cs.overflowX) || ["auto", "scroll"].includes(cs.overflowY)) break;
      const cutX = hx && (tr.right > ar.right + 1.5 || tr.left < ar.left - 1.5);
      const cutY = hy && ar.height > 2 && (tr.bottom > ar.bottom + 2 || tr.top < ar.top - 2) && cs.webkitLineClamp === "none";
      if (!cutX && !cutY) continue;
      if (seen.has(a)) break;
      seen.add(a);
      const ellipsis = cs.textOverflow === "ellipsis" && ["nowrap", "pre"].includes(cs.whiteSpace);
      if (cutX && ellipsis) { if (!reachable(a)) out.push({ kind: "truncated with no full text", what: name(a), d: Math.round(tr.right - ar.right) }); }
      else out.push({ kind: cutX ? "clipped across" : "clipped down", what: name(a), d: Math.round(cutX ? Math.max(tr.right - ar.right, ar.left - tr.left) : tr.bottom - ar.bottom) });
      break;
    }
  }
  return { out };
})()`;

// ── run ──────────────────────────────────────────────────────────────────────────────────
const fails = new Map(); // kind -> [lines]
let checked = 0, failed = 0, before = null;
const note = (kind, line) => { if (!fails.has(kind)) fails.set(kind, []); fails.get(kind).push(line); };

for (const dev of DEVICES) {
  await cdp("Emulation.setDeviceMetricsOverride", { width: dev.w, height: dev.h, deviceScaleFactor: 2, mobile: dev.touch });
  await cdp("Emulation.setTouchEmulationEnabled", { enabled: dev.touch, maxTouchPoints: dev.touch ? 5 : 1 });
  await cdp("Emulation.setEmulatedMedia", { features: [{ name: "prefers-reduced-motion", value: "reduce" }] });
  await cdp("Page.navigate", { url: BASE });
  await sleep(400);
  await js(`localStorage.clear(); localStorage.setItem("wado.settings", ${JSON.stringify(SETTINGS)}); localStorage.setItem("wado.client", "Phone-key"); true`);
  let devFails = 0;
  for (const view of VIEWS.filter((v) => v.name.includes(VIEW_ONLY))) {
    const opened = await view.open().catch((e) => (console.log(`  ..  ${dev.name}: ${view.name}: could not open (${e.message})`), false));
    if (opened === false) { note("view did not open", `${dev.name} · ${view.name}`); failed++; devFails++; continue; }
    if (before === null) before = !(await js(HAS_SIZE));
    for (const iface of before ? ["cozy"] : INTERFACES) {
      for (const text of TEXTS) {
        await applySize(text, iface);
        await settle();
        const r = await js(MEASURE(view.root, dev.touch));
        checked++;
        const tag = `${dev.name} ${dev.w}×${dev.h} · ${view.name} · text ${text}% · ${iface}`;
        if (r.missing) { note("view not found", tag); failed++; devFails++; continue; }
        if (r.out.length) { failed++; devFails++; }
        for (const f of r.out) note(f.kind, `${tag}: ${f.what} (${f.d}px)`);
        // The Computer page (and Appearance) at 390×844 — cozy 100%, compact 90%, large 140% — and at 1440×900.
        // (A build from before is shot at 100% and at 130%, the WebView's textZoom on a phone set
        // to a large font.)
        const shots = before ? [[390, "cozy", 100], [390, "cozy", 130]]
          : [[390, "cozy", 100], [390, "compact", 90], [390, "large", 140], [1440, "cozy", 100]];
        for (const [sw, si, st] of view.shoot ? shots : []) {
          if (sw !== dev.w || si !== iface || !TEXTS.includes(st) && text !== TEXTS[TEXTS.length - 1]) continue;
          if (TEXTS.includes(st) ? st !== text : text !== TEXTS[TEXTS.length - 1]) continue;
          if (!TEXTS.includes(st)) { await applySize(st, si); await settle(); }
          const s = await cdp("Page.captureScreenshot", { format: "png" });
          writeFileSync(join(OUT, `${view.name.replace(/\W+/g, "-").replace(/^-|-$/g, "")}-${dev.w}x${dev.h}-${before ? "before-" : ""}${si}-${st}.png`), Buffer.from(s.result.data, "base64"));
        }
      }
    }
    await applySize(100, "cozy");
  }
  console.log(`${devFails ? "FAIL" : "  ok"}  ${dev.name} ${dev.w}×${dev.h}${devFails ? ` — ${devFails} measured states with failures` : ""}`);
}

console.log(`\n${before ? "BEFORE (no size settings: text scaled like the WebView's textZoom, cozy only)" : "AFTER"} — ${checked} states measured, ${failed} with failures`);
for (const [kind, lines] of fails) {
  const uniq = [...new Set(lines)];
  console.log(`\n${kind}: ${lines.length}`);
  for (const l of VERBOSE ? uniq : uniq.slice(0, 8)) console.log("    " + l);
  if (!VERBOSE && uniq.length > 8) console.log(`    … ${uniq.length - 8} more`);
}
console.log(`\nscreenshots: ${OUT}`);
cleanup();
process.exit(failed ? 1 : 0);
