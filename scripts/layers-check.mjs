// Layers: one view at a time, every page and sheet opaque, sheets on a scrim, and nothing from a
// page underneath ever painted through or under the finger. In a real browser, with no computer
// behind the page (`?fixture`; the files channel is played by scripts/lib/files-fake.mjs).
//
//   dx build -p wado-client --platform web
//   node scripts/layers-check.mjs [out-dir]      (needs Chrome; run with the sandbox off)
//
// Per size (390×844, 1440×900, 2000×1270) it walks: home → a computer's page → one of its
// settings pages → back → back; a settings page of this phone; Files over a computer's page → a
// picture in the viewer → its details; the trackpad's settings sheet; the passkey sheet; and a
// session: the console, full screen, the control centre and a page pushed in it. After every
// step the animations are finished, then the page is sampled on a grid:
//
//   • painted: walking down the layers under each point, an opaque layer stops the walk; a layer
//     that is not opaque there lets the next one show through — that is a bleed, and fails
//   • exactly one full page is visible (under a sheet, the page its scrim dims)
//   • every page and sheet is opaque (its own background, or a ground covering it); a sheet or
//     modal sits on a scrim that covers the screen and catches the finger
//   • hit-testing: `elementFromPoint` never lands in a layer under the top one
//   • a covered page is not painted, or is inert
//
// Screenshots of each step go to out-dir.

import { spawn } from "node:child_process";
import { mkdirSync, mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { FILES_FAKE } from "./lib/files-fake.mjs";

const ROOT = new URL("..", import.meta.url).pathname;
const PUB = process.env.WADO_PUB || join(ROOT, "target/dx/wado-client/debug/web/public");
const OUT = process.argv[2] || join(tmpdir(), "wado-layers-shots");
mkdirSync(OUT, { recursive: true });
const T = mkdtempSync(join(tmpdir(), "wado-layers-"));
const HTTP = 8779, CDP = 9379;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const procs = [];
let failures = 0, passes = 0;
const ok = (c, name, d = "") => { if (!c) failures++; else passes++; console.log(`${c ? "  ok  " : "FAIL  "}${name}${c || !d ? "" : "\n        " + d}`); };
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
// Run every animation and transition to its end — twice, since the end of one starts the next.
const settle = async () => {
  for (let i = 0; i < 3; i++) {
    await sleep(160);
    await js(`document.getAnimations().forEach(a => { try { if (a.effect && a.effect.getComputedTiming().iterations !== Infinity) a.finish(); } catch {} }); true`);
  }
  await sleep(120);
};
const shot = async (name) => {
  const r = await cdp("Page.captureScreenshot", { format: "png" });
  writeFileSync(join(OUT, name + ".png"), Buffer.from(r.result.data, "base64"));
};
const click = (sel) => js(`(() => { const b = document.querySelector(${JSON.stringify(sel)}); if (!b) return false; b.click(); return true; })()`);

await cdp("Runtime.enable");
await cdp("Page.enable");
const BASE = `http://127.0.0.1:${HTTP}/`;

// ── the inspector, run in the page ────────────────────────────────────────────────────────
//
// A layer is a page (it should cover the screen and be opaque) or a sheet (an opaque panel on a
// scrim). `top` names the layer the person should be looking at.
const INSPECT = String.raw`((top) => {
  const LAYERS = [
    { name: "stage", sel: "#stage", page: true, base: true },
    { name: "home", sel: "#landing", page: true },
    { name: "computer", sel: "#computer", page: true },
    { name: "sub", sel: "#sub", page: true },
    { name: "files", sel: "#wado-files", panel: ".fpanel", scrim: "#wado-files" },
    { name: "viewer", sel: ".fview", page: true },
    { name: "viewer-details", sel: ".fvdetails.up:not(.side)", scrim: ".fvdscrim" },
    { name: "trackpad", sel: "#wado-trackpad", page: true },
    { name: "pad-settings", sel: ".tpsheet:not([hidden])", scrim: ".tpscrim:not([hidden])" },
    { name: "cc", sel: "#cc.open", scrim: ".ccscrim.open" },
    { name: "passkey", sel: ".pksheet.up", scrim: ".pkscrim.up" },
    { name: "widget-sheet", sel: ".wsheet", scrim: ".wscrim" },
    { name: "window-sheet", sel: ".winsheet", scrim: ".scrim" },
    { name: "dialog", sel: ".fmodal .fdialog", scrim: ".fmodal" },
  ];
  // Things that float over every layer by design: status and notices, never a page.
  const CHROME = ".toasts, .toast, .linkbadge, #wado-files-pill, .ftoast, #clipdock, #wado-here, #wado-osk, #wado-presence";
  const W = innerWidth, H = innerHeight;
  const alpha = (c) => { const m = /rgba?\(([^)]+)\)/.exec(c); if (!m) return c === "transparent" ? 0 : 1; const p = m[1].split(/[ ,/]+/).filter(Boolean); return p.length > 3 ? parseFloat(p[3]) : 1; };
  // How much of an element is painted: 0 when it or an ancestor is not displayed or hidden.
  const paint = (el) => {
    if (!el || !el.isConnected) return 0;
    const cs = getComputedStyle(el);
    if (cs.visibility !== "visible" || cs.display === "none") return 0;
    let o = 1;
    for (let e = el; e && e.nodeType === 1; e = e.parentElement) {
      const s = getComputedStyle(e);
      if (s.display === "none" || e.hidden) return 0;
      o *= parseFloat(s.opacity);
    }
    const r = el.getBoundingClientRect();
    if (r.width < 1 || r.height < 1 || r.right <= 0 || r.bottom <= 0 || r.left >= W || r.top >= H) return 0;
    return o;
  };
  const solid = (el) => { const s = getComputedStyle(el); const a = alpha(s.backgroundColor); return a >= 0.99 || (a >= 0.88 && s.backdropFilter && s.backdropFilter !== "none"); };
  const inside = (r, x, y) => x >= r.left && x < r.right && y >= r.top && y < r.bottom;
  // The opaque surfaces of a layer: itself, or descendants that are painted solid (a ground).
  const grounds = (el) => {
    const out = [];
    if (paint(el) > 0.99 && solid(el)) out.push(el.getBoundingClientRect());
    for (const d of el.querySelectorAll("*")) {
      const r = d.getBoundingClientRect();
      if (r.width * r.height < W * H * 0.25) continue;
      if (solid(d) && paint(d) > 0.99) out.push(r);
    }
    return out;
  };
  // Where a layer paints: the z-index of its outermost positioned ancestor that has one (the
  // stacking context it competes in at the root), then document order.
  const z = (el) => {
    let v = 0;
    for (let e = el; e && e.nodeType === 1; e = e.parentElement) {
      const s = getComputedStyle(e), n = parseInt(s.zIndex);
      if (!isNaN(n) && s.position !== "static") v = n;
    }
    return v;
  };
  const present = [];
  for (const L of LAYERS) {
    for (const el of document.querySelectorAll(L.sel)) {
      const panel = L.panel ? el.querySelector(L.panel) : el;
      if (!panel) continue;
      // Its scrim: itself, a sibling, or the first in the page.
      const scrim = !L.scrim ? null : L.scrim === L.sel ? el
        : (el.parentElement && [...el.parentElement.children].find((c) => c.matches(L.scrim))) || document.querySelector(L.scrim);
      const p = paint(panel);
      const r = panel.getBoundingClientRect();
      const full = r.left <= 1 && r.top <= 1 && r.right >= W - 1 && r.bottom >= H - 1;
      present.push({ ...L, el, panel, scrim, p, full, z: z(el), inert: !!el.closest("[inert]"), ground: p > 0 ? grounds(panel) : [] });
    }
  }
  // Paint order of the layers: z-index, then document order.
  const order = (a, b) => a.z - b.z || (a.el.compareDocumentPosition(b.el) & Node.DOCUMENT_POSITION_FOLLOWING ? -1 : 1);
  const painted = present.filter((l) => l.p > 0.02).sort(order).reverse();
  const T = present.find((l) => l.name === top);
  const res = { top, ok: !!T && T.p > 0.5, live: !!T && !T.inert, painted: painted.map((l) => l.name + (l.full ? "" : "·part") + "@" + l.p.toFixed(2)), bleed: [], visiblePages: [], opaque: [], scrim: [], hits: [], inert: [] };
  if (!T) return res;
  // Sample on a grid and walk down the layers at each point.
  const seen = new Set(), reached = new Set();
  const bleeds = new Map();
  const N = 9, M = 13;
  for (let i = 0; i < N; i++) for (let j = 0; j < M; j++) {
    const x = Math.round((i + 0.5) * W / N), y = Math.round((j + 0.5) * H / M);
    let over = null, dimmed = false;
    for (const l of painted) {
      if (!inside(l.panel.getBoundingClientRect(), x, y) && !(l.scrim && paint(l.scrim) > 0.02 && inside(l.scrim.getBoundingClientRect(), x, y))) continue;
      const onPanel = inside(l.panel.getBoundingClientRect(), x, y);
      if (!onPanel) { // its scrim: it dims what is under it
        if (alpha(getComputedStyle(l.scrim).backgroundColor) * paint(l.scrim) >= 0.3) dimmed = true;
        continue;
      }
      const opaque = l.p > 0.99 && l.ground.some((r) => inside(r, x, y));
      if (over && !over.opaque) {
        const k = over.name + " over " + l.name;
        bleeds.set(k, (bleeds.get(k) || 0) + 1);
      }
      if (l.page || l.full) seen.add(l.name);
      reached.add(l);
      over = { name: l.name, opaque };
      if (opaque) break;
    }
    // Hit-testing: the finger lands in the top layer at this point, its scrim, or chrome.
    const hit = document.elementFromPoint(x, y);
    if (hit && !hit.closest(CHROME)) {
      const owner = present.filter((l) => l.el.contains(hit) || (l.scrim && l.scrim.contains(hit))).sort(order).reverse()[0];
      const topHere = painted.find((l) => inside(l.panel.getBoundingClientRect(), x, y) || (l.scrim && paint(l.scrim) > 0.02 && inside(l.scrim.getBoundingClientRect(), x, y)));
      if (owner && topHere && owner !== topHere && !owner.base) res.hits.push(owner.name + " under " + topHere.name + " @" + x + "," + y);
    }
  }
  res.bleed = [...bleeds].map(([k, n]) => k + " ×" + n);
  res.visiblePages = [...seen].filter((n) => present.find((l) => l.name === n && (l.page || l.full)));
  // Opaque: the top layer and every sheet painted.
  for (const l of painted) {
    if (l.base) continue;
    const r = l.panel.getBoundingClientRect();
    const covered = l.ground.some((g) => g.left <= r.left + 2 && g.top <= Math.max(0, r.top) + 2 && g.right >= Math.min(W, r.right) - 2 && g.bottom >= Math.min(H, r.bottom) - 2);
    if (l.p > 0.99 && !covered && !(l.page && !l.full)) res.opaque.push(l.name);
    // A sheet that is not the whole screen sits on a scrim that covers the screen and is hit.
    if (!l.page && !l.full) {
      const s = l.scrim, sr = s && s.getBoundingClientRect();
      const good = s && paint(s) > 0 && getComputedStyle(s).pointerEvents !== "none" && sr.left <= 1 && sr.top <= 1 && sr.right >= W - 1 && sr.bottom >= H - 1;
      if (!good) res.scrim.push(l.name);
    }
  }
  // Covered: a layer painted but nowhere seen (an opaque layer is over all of it) is inert —
  // never tapped or focused through what covers it. One seen under a sheet's scrim is the page
  // that sheet belongs to, and stays as it is.
  for (const l of present) {
    if (l === T || l.base || l.p <= 0.02 || reached.has(l)) continue;
    if (!l.inert) res.inert.push(l.name);
  }
  return res;
})`;

async function check(tag, top, opts = {}) {
  await settle();
  const r = await js(`${INSPECT}(${JSON.stringify(top)})`);
  const where = `${tag} [painted: ${r.painted.join(", ")}]`;
  ok(r.ok, `${tag}: ${top} is on top`, where);
  // The layer on top takes taps and focus (Dioxus once wrote `inert="false"`, which is inert).
  if (!r.live && opts.knownInert) console.log(`KNOWN ${tag}: ${top} is inert while up — ${opts.knownInert}`);
  else ok(r.live, `${tag}: ${top} is live, not inert`);
  ok(!r.bleed.length, `${tag}: nothing shows through`, r.bleed.join("; "));
  const pages = r.visiblePages.filter((n) => n !== "stage" || opts.stage);
  ok(pages.length === 1, `${tag}: exactly one full page visible (${pages.join(", ") || "none"})`, where);
  ok(!r.opaque.length, `${tag}: every page and sheet is opaque`, r.opaque.join(", "));
  ok(!r.scrim.length, `${tag}: every sheet sits on a scrim`, r.scrim.join(", "));
  ok(!r.hits.length, `${tag}: nothing underneath is under the finger`, r.hits.slice(0, 6).join("; "));
  ok(!r.inert.length, `${tag}: covered layers are hidden or inert`, r.inert.join(", "));
  if (opts.shot !== false) await shot(tag.replace(/[^\w-]+/g, "_"));
  return r;
}

// Halfway through a transition (every animation paused at its middle), only the two pages it is
// between may be painted: never the one under the outgoing page.
async function midway(tag, allowed) {
  await sleep(50);
  const names = await js(`(() => {
    document.getAnimations().forEach((a) => { try { a.pause(); a.currentTime = (a.effect.getComputedTiming().activeDuration || 0) / 2; } catch {} });
    return ${INSPECT}("home").painted; })()`);
  const extra = names.map((n) => n.split(/[·@]/)[0]).filter((n) => n !== "stage" && !allowed.includes(n));
  ok(!extra.length, `${tag}: halfway, only ${allowed.join(" and ")} are painted`, names.join(", "));
  await shot((tag + " midway").replace(/[^\w-]+/g, "_"));
  await js(`document.getAnimations().forEach((a) => { try { a.play(); } catch {} }); true`);
}
// After a transition, no animation is left holding a page in place (`fill: forwards` that was
// never let go would beat every later state of that page).
async function nothingHeld(tag) {
  await sleep(300);
  const held = await js(`[...document.querySelectorAll("#landing, #computer, #sub")].flatMap((el) => el.getAnimations()
    .filter((a) => a.playState === "finished" && /forwards|both/.test(a.effect.getComputedTiming().fill)).map(() => el.id))`);
  ok(!held.length, `${tag}: no finished animation still holds a page`, held.join(", "));
}

const PROFILES = [
  { name: "bushido", host: "", conn_mode: "relay", relay_url: "http://10.255.255.1:9", remote_id: "123456789" },
  { name: "Framework", host: "", conn_mode: "relay", relay_url: "http://10.255.255.1:9", remote_id: "987654321" },
];

async function open(w, h, opts = {}) {
  const mobile = w < 700;
  await cdp("Emulation.setDeviceMetricsOverride", { width: w, height: h, deviceScaleFactor: mobile ? 2 : 1, mobile });
  await cdp("Emulation.setTouchEmulationEnabled", { enabled: mobile, maxTouchPoints: mobile ? 5 : 1 });
  const settings = JSON.stringify({ conn_mode: "relay", relay_url: PROFILES[0].relay_url, remote_id: PROFILES[0].remote_id, profile: 0,
    theme: opts.theme || "default-dark", profiles: PROFILES });
  await cdp("Page.navigate", { url: BASE });
  await sleep(400);
  await js(`localStorage.clear(); localStorage.setItem("wado.settings", ${JSON.stringify(settings)}); true`);
  await cdp("Page.navigate", { url: BASE + "?fixture" });
  await until("!!window.__wado && !!window.__wado.fixture && !!document.querySelector('#landing .ccard[data-key]')", 12000);
  await js(`(() => { const W = window.__wado; ${FILES_FAKE}; return true; })()`);
  await settle();
}

const openComputer = async () => {
  await click("#landing .ccard[data-key] .cpick");
  return until("!!document.getElementById('computer')", 4000);
};

for (const [w, h] of [[390, 844], [1440, 900], [2000, 1270]]) {
  const sz = `${w}x${h}`;
  console.log(`\n── ${sz}`);
  await open(w, h);
  await check(`${sz} home`, "home");

  ok(await openComputer(), `${sz}: a computer's page opens`);
  await check(`${sz} computer`, "computer");
  // Scrolled: the ground has to hold all the way down, not only over the first screen.
  await js(`(() => { const c = document.getElementById("computer"); c.scrollTop = c.scrollHeight; return true; })()`);
  await check(`${sz} computer scrolled`, "computer");
  await js(`(() => { document.getElementById("computer").scrollTop = 0; return true; })()`);

  await click("#computer .settings .navrow");
  ok(await until("!!document.getElementById('sub')", 3000), `${sz}: its settings page opens`);
  await check(`${sz} computer settings`, "sub");
  await click("#sub .phead button");
  await midway(`${sz} settings → computer`, ["sub", "computer"]);
  ok(await until("!document.getElementById('sub')", 3000), `${sz}: back to the computer`);
  await check(`${sz} computer again`, "computer", { shot: false });
  await nothingHeld(`${sz} computer again`);

  // Files from the computer's page, then a picture, then its details.
  await js(`window.__wado.files.openUi(); true`);
  ok(await until("!!document.querySelector('#wado-files .fpanel .fmain *')", 4000), `${sz}: Files opens`);
  await check(`${sz} files`, "files");
  await js(`(() => { const F = window.__wado.files; const e = F.fakeEntries(); F.view(e.find((x) => x.name.endsWith(".png")), e); return true; })()`);
  ok(await until("!!document.querySelector('.fview img')", 4000), `${sz}: a picture opens in the viewer`);
  await check(`${sz} viewer`, "viewer");
  // Its details: a sheet on a scrim on a phone (the picture and its controls inert under it), a
  // panel beside the picture on a wide screen (never over it).
  await click(".fview .fvdbtn");
  ok(await until("!!document.querySelector('.fvdetails.up')", 3000), `${sz}: the details open`);
  if (w < 900) {
    await check(`${sz} viewer details`, "viewer-details");
    ok(await js(`[...document.querySelectorAll(".fview .fvstage, .fview .fvfoot")].every((n) => n.inert)`), `${sz}: under the details sheet, the picture and its controls are inert`);
  } else {
    await check(`${sz} viewer details`, "viewer");
    const r = await js(`(() => { const a = document.querySelector(".fvdetails.up").getBoundingClientRect(), b = document.querySelector(".fview .fvstage").getBoundingClientRect();
      return { overlap: a.left < b.right - 1 && b.left < a.right - 1 && a.top < b.bottom - 1 && b.top < a.bottom - 1, w: a.width }; })()`);
    ok(!r.overlap && r.w > 300, `${sz}: the details are a panel beside the picture, not over it`, JSON.stringify(r));
  }
  await js(`document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })); true`);
  ok(await until("!document.querySelector('.fvdetails.up') && !!document.querySelector('.fview')", 2000), `${sz}: Escape closes the details first, the viewer stays`);
  await js(`(() => { const F = window.__wado.files; F.viewClose(); F.closeUi(); return true; })()`);
  ok(await until("!document.querySelector('.fview')", 2000), `${sz}: the viewer is gone from the page once closed`);
  await check(`${sz} computer after files`, "computer", { shot: false });

  await click("#computer .cbar button");
  await midway(`${sz} computer → home`, ["computer", "home"]);
  ok(await until("!document.getElementById('computer')", 3000), `${sz}: back home`);
  await check(`${sz} home again`, "home", { shot: false });
  await nothingHeld(`${sz} home again`);

  // A setting of this phone, pushed over home.
  await click("#landing .navrow");
  ok(await until("!!document.getElementById('sub')", 3000), `${sz}: a phone setting opens`);
  await check(`${sz} phone settings`, "sub");
  await click("#sub .phead button");
  await until("!document.getElementById('sub')", 3000);

  // The passkey sheet over home.
  await js(`window.__wado.fixture({ type: "passkey", state: { registered: true, unlocked: false, origin: "" }, busy: false, need: "pad", note: "", supported: true, platform: true }); true`);
  if (await until("!!document.querySelector('.pksheet.up')", 2000)) await check(`${sz} passkey`, "passkey",
    { knownInert: "ui/passkey/mod.rs writes `inert: !up`, which Dioxus renders as inert=\"false\"; use `inert: crate::ui::widgets::inert(!up)` (left to the passkey work)" });
  else ok(false, `${sz}: the passkey sheet opens`);
  await js(`window.__wado.passkeyCancel && window.__wado.passkeyCancel(); true`);

  // The trackpad's settings sheet, opened from a computer's page.
  await openComputer();
  await settle();
  if (await js(`!!(window.__wado.padSettings && window.__wado.padSettings.openHere)`)) {
    await js(`window.__wado.padSettings.openHere(); true`);
    if (await until("!!document.querySelector('.tpsheet:not([hidden])')", 2000)) await check(`${sz} trackpad settings`, "pad-settings");
    else ok(false, `${sz}: the trackpad settings sheet opens`);
    await js(`(() => { const s = document.querySelector('.tpscrim'); if (s) s.click(); return true; })()`);
    await settle();
  }

  // A session: the console, the cockpit, the control centre and a page pushed in it.
  await js(`(() => { const W = window.__wado; W.fixture({ type: "sessionOn" });
    W.fixture({ type: "link", state: "direct", path: "lan", rtt_ms: 23, net: "wifi" }); return true; })()`);
  ok(await until("!!document.getElementById('wado-video') && document.documentElement.dataset.arr === 'console'", 6000), `${sz}: a session opens as a console`);
  await check(`${sz} console`, "stage", { stage: true });
  await click("[data-arrange='toggle']");
  await until("document.documentElement.dataset.arr === 'cockpit'", 3000);
  await check(`${sz} cockpit`, "stage", { stage: true, shot: false });
  await click("#wado-bar .barbtn.more");
  ok(await until("!!document.querySelector('#cc.open')", 3000), `${sz}: the control centre opens`);
  await check(`${sz} control centre`, "cc", { stage: true });
  await click("#cc .page .navrow");
  if (await until("!!document.querySelector('#cc .page.pushed')", 2000)) {
    await check(`${sz} control centre page`, "cc", { stage: true });
    const behind = await js(`(() => { const b = document.querySelector('#cc .page.behind'); if (!b) return null; const s = getComputedStyle(b);
      return { vis: s.visibility, op: s.opacity, inert: !!b.closest('[inert]') }; })()`);
    ok(!behind || behind.vis === "hidden" || (+behind.op === 0 && behind.inert), `${sz}: the control centre's root under a pushed page is hidden`, JSON.stringify(behind));
  }
}

cleanup();
console.log(`\n${passes} passed, ${failures} failed — shots in ${OUT}`);
process.exit(failures ? 1 : 0);
