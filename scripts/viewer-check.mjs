// The file viewer in a real browser, with no computer behind the page: the files channel is played
// by scripts/lib/files-fake.mjs, so every kind of file can be opened and drawn.
//
//   dx build -p wado-client --platform web
//   node scripts/viewer-check.mjs [out-dir]      (needs Chrome; run with the sandbox off)
//
// Per size (390×844, 1440×900) and theme (default-dark, catppuccin-latte), for every kind the
// viewer knows: it opens over Files on an opaque layer, draws the right thing (a picture that
// decoded, a player, lines with numbers, a table, a listing, a sample in the font, an honest
// card), the name keeps its extension, every control is ≥ 44 px, and the details open as a sheet
// (phone) or a panel (wide). Then: the overflow menu, the wrap toggle, "Show more", the size cap,
// HTML shown as source and SVG never run, the relay's limited lane, prev/next, Escape.
// A screenshot of each kind and of the details goes to out-dir.

import { spawn, execFileSync } from "node:child_process";
import { mkdirSync, mkdtempSync, writeFileSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { FILES_FAKE } from "./lib/files-fake.mjs";

const ROOT = new URL("..", import.meta.url).pathname;
const PUB = process.env.WADO_PUB || join(ROOT, "target/dx/wado-client/debug/web/public");
const OUT = process.argv[2] || join(tmpdir(), "wado-viewer-shots");
mkdirSync(OUT, { recursive: true });
const T = mkdtempSync(join(tmpdir(), "wado-viewer-"));
const HTTP = 8781, CDP = 9381;
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
const errors = [];
ws.onmessage = (e) => {
  const m = JSON.parse(e.data);
  if (m.id && pend.has(m.id)) { pend.get(m.id)(m); pend.delete(m.id); }
  if (m.method === "Runtime.exceptionThrown") errors.push(m.params.exceptionDetails.exception?.description || m.params.exceptionDetails.text);
};
const cdp = (method, params = {}) => new Promise((r) => { const i = ++id; pend.set(i, r); ws.send(JSON.stringify({ id: i, method, params })); });
const js = async (expr) => {
  const r = await cdp("Runtime.evaluate", { expression: expr, returnByValue: true, awaitPromise: true });
  if (r.result?.exceptionDetails) throw new Error(r.result.exceptionDetails.exception?.description || "evaluate failed");
  return r.result?.result?.value;
};
const until = async (expr, ms = 8000) => { const t = Date.now(); while (Date.now() - t < ms) { if (await js(expr).catch(() => false)) return true; await sleep(120); } return false; };
const settle = async () => {
  for (let i = 0; i < 2; i++) {
    await sleep(150);
    await js(`document.getAnimations().forEach(a => { try { if (a.effect && a.effect.getComputedTiming().iterations !== Infinity) a.finish(); } catch {} }); true`);
  }
  await sleep(100);
};
const shot = async (name) => {
  await settle();
  const r = await cdp("Page.captureScreenshot", { format: "png" });
  writeFileSync(join(OUT, name + ".png"), Buffer.from(r.result.data, "base64"));
};
await cdp("Runtime.enable");
await cdp("Page.enable");
const BASE = `http://127.0.0.1:${HTTP}/`;

async function open(w, h, theme) {
  const mobile = w < 700;
  await cdp("Emulation.setDeviceMetricsOverride", { width: w, height: h, deviceScaleFactor: mobile ? 2 : 1, mobile });
  await cdp("Emulation.setTouchEmulationEnabled", { enabled: mobile, maxTouchPoints: mobile ? 5 : 1 });
  const settings = JSON.stringify({ conn_mode: "relay", relay_url: "http://10.255.255.1:9", remote_id: "123456789", profile: 0, theme,
    profiles: [{ name: "bushido", host: "", conn_mode: "relay", relay_url: "http://10.255.255.1:9", remote_id: "123456789" }] });
  await cdp("Page.navigate", { url: BASE });
  await sleep(400);
  await js(`localStorage.clear(); localStorage.setItem("wado.settings", ${JSON.stringify(settings)}); true`);
  await cdp("Page.navigate", { url: BASE + "?fixture" });
  await until("!!window.__wado && !!window.__wado.files && !!window.__wado.files.viewer", 12000);
  await js(`(() => { window.__fakeFont = ${JSON.stringify(FONT)}; const W = window.__wado; ${FILES_FAKE}; W.files.openUi(); return true; })()`);
  await until("!!document.querySelector('#wado-files .fmain *')", 6000);
  // Panzoom and Plyr come from a CDN (SRI-pinned in index.html): the zoom tools need the first.
  await until("!!window.Panzoom && !!window.Plyr", 10000);
}
// A real font from this machine for the font sample, when there is one (fontconfig).
let FONT = "";
try {
  const f = execFileSync("fc-list", [":fontformat=TrueType", "file"]).toString().split("\n").map((l) => l.replace(/:\s*$/, "").trim()).find((l) => /\.ttf$/i.test(l));
  if (f) FONT = readFileSync(f).toString("base64");
} catch {}
const view = (name) => js(`(() => { const F = window.__wado.files; const e = F.fakeEntries(); F.view(e.find((x) => x.name === ${JSON.stringify(name)}), e); return true; })()`);

// What each kind must have drawn, as an expression over the stage.
const KINDS = [
  ["cristiano-ronaldo-3840x2160-14055.png", "png", `(document.querySelector(".fview .fvimg") || {}).naturalWidth === 1920 && !!document.querySelector(".fview .fvtools [aria-label='Zoom in']")`],
  ["IMG_2041.jpg", "jpg", `(document.querySelector(".fview .fvimg") || {}).naturalWidth === 900`],
  ["sunset.webp", "webp", `(document.querySelector(".fview .fvimg") || {}).naturalWidth === 1200`],
  ["diagram.svg", "svg", `(document.querySelector(".fview .fvimg") || {}).naturalWidth > 0 && document.title !== "pwned" && !document.querySelector(".fview svg rect")`],
  ["IMG_5520.heic", "heic", `!!document.querySelector(".fview .fvcardx") && /heic/.test(document.querySelector(".fview .fvcardtitle").textContent) && !!document.querySelector(".fview .fvthumbimg")`],
  ["holiday.mp4", "video", `(() => { const v = document.querySelector(".fview video"); return !!v && v.readyState >= 2; })()`],
  ["voice-memo.wav", "audio", `(() => { const a = document.querySelector(".fview audio"); return !!a && a.duration > 1.5 && (document.querySelector(".fview .fpcover")?.textContent || "").includes("voice-memo.wav"); })()`],
  ["report.pdf", "pdf", `!!document.querySelector(".fview iframe.fvpdf") || /PDF app/.test(document.querySelector(".fview .fvcardtitle")?.textContent || "")`],
  ["trip-notes.md", "markdown", `!!document.querySelector(".fview .fvmd h2") && !!document.querySelector(".fview .fvmd strong") && !!document.querySelector(".fview .fvmd li") && !!document.querySelector(".fview .fvmd pre")`],
  ["main.rs", "code", `document.querySelectorAll(".fview .fvcode .fvl").length > 80 && !!document.querySelector(".fview .fvcode .tc") && !!document.querySelector(".fview .fvcode .ts")`],
  ["package.json", "json", `(document.querySelector(".fview .fvtext")?.textContent || "").includes('\\n  "scripts": {')`],
  ["weather.csv", "csv", `document.querySelectorAll(".fview .fvtable tbody tr").length === 240 && document.querySelectorAll(".fview .fvtable th").length === 5`],
  ["index.html", "html", `(document.querySelector(".fview .fvtext")?.textContent || "").includes("<script>alert(1)</script>")`],
  ["photos.zip", "zip", `document.querySelectorAll(".fview .fvalist li").length === 5`],
  ["backup.tar.gz", "tar", `/\\.gz/.test(document.querySelector(".fview .fvcardtitle")?.textContent || "")`],
  ["Inter-Regular.ttf", "font", FONT ? `!!document.querySelector(".fview .fvfont .fvf2")` : `/could not read this font/.test(document.querySelector(".fview .fvcardx p")?.textContent || "")`],
  ["firmware.bin", "binary", `/Can't preview \\.bin here/.test(document.querySelector(".fview .fvcardtitle")?.textContent || "") && /Size/.test(document.querySelector(".fview .fvfacts")?.textContent || "")`],
  ["server.log", "log", `document.querySelectorAll(".fview .fvcode .fvl").length === 1500 && !document.querySelector(".fview .fvmorebtn").hidden`],
];

// The fake answers `entries` for the zip like the daemon would.
const ZIP_ENTRIES = `(() => { const F = window.__wado.files, req = F.req; F.req = (op, a) => op === "entries"
  ? Promise.resolve({ entries: [{ name: "photos/", size: 0, dir: true }, { name: "photos/beach.jpg", size: 3000, dir: false }, { name: "photos/hills.jpg", size: 5200, dir: false },
    { name: "notes.txt", size: 6, dir: false }, { name: "src/main.rs", size: 2100, dir: false }], total: 5, more: false }) : req(op, a); return true; })()`;

const TARGETS = `(() => { const bad = [];
  for (const b of document.querySelectorAll(".fview button, .fview a.fvaction")) {
    if (b.closest("[inert], .plyr, .fpanel-vlc") || !b.offsetParent) continue;
    const r = b.getBoundingClientRect(); if (r.width < 1) continue;
    if (b.matches(".fvlink") ? r.height < 24 : (r.height < 43.5 || r.width < 43.5)) bad.push((b.getAttribute("aria-label") || b.textContent).trim().slice(0, 24) + " " + Math.round(r.width) + "×" + Math.round(r.height));
  } return bad; })()`;

for (const theme of ["default-dark", "catppuccin-latte"]) {
  for (const [w, h] of [[390, 844], [1440, 900]]) {
    const sz = `${w}x${h}`, tag = `${sz} ${theme}`;
    console.log(`\n── ${tag}`);
    await open(w, h, theme);
    await js(ZIP_ENTRIES);
    for (const [name, label, drawn] of KINDS) {
      await view(name);
      const okDrawn = await until(drawn, 9000);
      ok(okDrawn, `${tag} ${label}: drawn`, await js(`document.querySelector(".fview .fvstage")?.innerText?.slice(0, 160)`));
      await settle();
      const s = await js(`(() => { const v = document.querySelector(".fview"), cs = getComputedStyle(v), t = document.querySelector(".fview .fvtitle b");
        const tail = t.querySelector(".fvnt").getBoundingClientRect(), box = t.getBoundingClientRect();
        return { n: document.querySelectorAll(".fview").length, bg: cs.backgroundColor, ext: tail.right <= box.right + 1 && tail.width > 0, files: document.getElementById("wado-files").inert }; })()`);
      ok(s.n === 1 && !/rgba\(0, 0, 0, 0\)|transparent/.test(s.bg) && s.files, `${tag} ${label}: one opaque layer, Files inert under it`, JSON.stringify(s));
      ok(s.ext, `${tag} ${label}: the name's end and extension show`);
      const small = await js(TARGETS);
      ok(!small.length, `${tag} ${label}: every control ≥ 44 px`, small.join(", "));
      await shot(`${sz}-${theme}-${label}`);
    }
    // Details for a picture: the sheet on a phone, the panel when wide.
    await view("IMG_2041.jpg");
    await until(`(document.querySelector(".fview .fvimg") || {}).naturalWidth === 900`);
    await js(`document.querySelector(".fview .fvdbtn").click(); true`);
    ok(await until(`!!document.querySelector(".fvdetails.up") && /900 × 1200/.test(document.querySelector(".fvmeta").textContent)`, 3000), `${tag}: details open with the picture's facts`);
    ok(await js(w < 900 ? `document.querySelector(".fvdetails.up").classList.contains("side") === false && document.querySelector(".fview .fvfoot").inert`
      : `document.querySelector(".fvdetails.up").classList.contains("side")`), `${tag}: details are a ${w < 900 ? "sheet (the footer inert under it)" : "panel beside the picture"}`);
    await shot(`${sz}-${theme}-details`);
    await js(`document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })); true`);
    ok(await until(`!document.querySelector(".fvdetails.up") && !!document.querySelector(".fview")`, 2000), `${tag}: Escape puts the details away, the viewer stays`);
    // The overflow menu: every action in one place, keyboard-friendly.
    await js(`document.querySelector(".fview .fvmore").click(); true`);
    const items = await js(`[...document.querySelectorAll(".fvmenu .fvmenuitem")].map((b) => b.textContent)`);
    ok(["Details", "Download", "Copy path", "Open folder"].every((x) => items.includes(x)) && await js(`document.activeElement.classList.contains("fvmenuitem")`),
      `${tag}: ⋯ holds the actions, focus on the first`, items.join(", "));
    await shot(`${sz}-${theme}-menu`);
    await js(`document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })); true`);
    ok(await until(`!document.querySelector(".fvmenu") && document.activeElement.classList.contains("fvmore")`, 2000), `${tag}: Escape closes the menu, focus back on ⋯`);
    if (theme !== "default-dark") continue;
    // Text: wrap toggle, show more.
    await view("server.log");
    await until(`document.querySelectorAll(".fview .fvcode .fvl").length === 1500`);
    await js(`document.querySelector(".fview .fvchip").click(); true`);
    ok(await js(`document.querySelector(".fview .fvcode").classList.contains("nowrap") && document.querySelector(".fview .fvchip").getAttribute("aria-pressed") === "false"`), `${tag}: Wrap turns off`);
    await js(`document.querySelector(".fview .fvchip").click(); document.querySelector(".fview .fvmorebtn").click(); true`);
    ok(await until(`document.querySelectorAll(".fview .fvcode .fvl").length === 3000`), `${tag}: Show more adds lines`);
    // Next / previous.
    const n0 = await js(`document.querySelector(".fvtitle b").textContent`);
    await js(`document.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowLeft", bubbles: true })); true`);
    ok(await until(`document.querySelector(".fvtitle b").textContent !== ${JSON.stringify(n0)}`, 3000), `${tag}: ← goes to the previous file`);
    if (w >= 900) {
      await js(`document.querySelector(".fvnav.next").click(); true`);
      ok(await until(`document.querySelector(".fvtitle b").textContent === ${JSON.stringify(n0)}`, 3000), `${tag}: the › arrow goes to the next`);
    }
    // The relay's limited lane: a big preview waits, a small one comes, a listing comes.
    await js(`window.__wado.files.fakeLane("fallback"); window.__wado.files.lane = "fallback"; window.__wado.files.viewerForget(); true`);
    const waits = `/waits for a direct connection/.test(document.querySelector(".fview .fvcardtitle")?.textContent || "")`;
    await view("sunset.webp");
    ok(await until(waits, 3000), `${tag}: limited lane — a picture over 1 MB waits for Direct, and says so`);
    await shot(`${sz}-${theme}-lane`);
    await view("holiday.mp4");
    ok(await until(waits, 3000) && !(await js(`!!document.querySelector(".fview video")`)), `${tag}: limited lane — a video does not stream over the relay`);
    await view("photos.zip");
    ok(await until(`document.querySelectorAll(".fview .fvalist li").length === 5`, 3000), `${tag}: limited lane — an archive's listing still comes`);
    await view("package.json");
    ok(await until(`(document.querySelector(".fview .fvtext")?.textContent || "").includes('"scripts"')`, 4000), `${tag}: limited lane — a small file still opens`);
    await js(`window.__wado.files.lane = "direct"; true`);
    await js(`document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })); true`);
    ok(await until(`!document.querySelector(".fview") && !document.getElementById("wado-files").inert`, 2000), `${tag}: Escape closes the viewer; Files is live again`);
  }
}
ok(!errors.length, "no script errors", errors.slice(0, 5).join("\n        "));
cleanup();
console.log(`\n${passes} passed, ${failures} failed — shots in ${OUT}`);
process.exit(failures ? 1 : 0);
