// The installed app in a real browser: install criteria, offline launch, signed updates and the
// home-screen shortcuts. Serves a built client at http://localhost:8770/wado/ (as Pages does) and
// drives headless Chrome over DevTools on port 9339. Needs no daemon: a computer's answers are
// played through the page's `?fixture` hook. Uses a throwaway DEV key (release-sign.mjs dev).
// Both modes: no release key set up (updates taken unsigned), then a key (only signed ones).
//
//   dx build -p wado-client --release --platform web --base-path wado
//   node scripts/app-e2e.mjs [built-tree] [--shots <dir>]
//
// Run with the Bash sandbox off (Chrome). It kills only the Chrome it started.
import { execFileSync, spawn } from "node:child_process";
import { cpSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync, existsSync, statSync, appendFileSync } from "node:fs";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { extname, join, resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const args = process.argv.slice(2);
const shotsAt = args.indexOf("--shots");
const SHOTS = shotsAt >= 0 ? resolve(args[shotsAt + 1]) : mkdtempSync(join(tmpdir(), "wado-app-shots-"));
const BUILT = resolve(args.find((a, i) => !a.startsWith("--") && i !== shotsAt + 1) ||
  join(ROOT, "target/dx/wado-client/release/web/public"));
const HTTP = 8770, DEBUG = 9339;
const BASE = `http://localhost:${HTTP}/wado/`;
// A second origin on the same server, for the no-key phase: its own worker, caches and storage.
const BASE2 = `http://127.0.0.1:${HTTP}/wado/`;
const WORK = mkdtempSync(join(tmpdir(), "wado-app-e2e-"));
mkdirSync(SHOTS, { recursive: true });

let failed = 0;
const ok = (cond, what) => { console.log((cond ? "ok   " : "FAIL ") + what); if (!cond) failed++; return cond; };
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

// ── trees: the pristine build, copied and released under a version each ──────────────────
// `nokey`: released as CI releases with no key set up — release.json only, sw-keys.js untouched.
function tree(name, { version, marker, unsigned, tamper, nokey } = {}) {
  const dir = join(WORK, name);
  cpSync(BUILT, dir, { recursive: true });
  if (marker) {
    const p = join(dir, "index.html");
    writeFileSync(p, readFileSync(p, "utf8").replace("</head>", `<!-- ${marker} --></head>`));
  }
  const a = nokey
    ? [join(ROOT, "scripts/release-sign.mjs"), "manifest", dir, "--version", version]
    : [join(ROOT, "scripts/release-sign.mjs"), "dev", dir, "--version", version];
  if (unsigned) a.push("--unsigned");
  execFileSync(process.execPath, a, { stdio: "ignore" });
  if (tamper) appendFileSync(join(dir, tamper), "tampered");
  return dir;
}

// ── a static server whose tree can be swapped, or stopped (offline) ──────────────────────
const TYPES = { ".html": "text/html", ".js": "text/javascript", ".css": "text/css", ".wasm": "application/wasm",
  ".png": "image/png", ".json": "application/json", ".webmanifest": "application/manifest+json", ".sig": "text/plain" };
let served = null;
const server = createServer((req, res) => {
  const path = decodeURIComponent(new URL(req.url, BASE).pathname);
  if (!path.startsWith("/wado/")) { res.writeHead(404).end(); return; }
  let f = join(served, path.slice(6) || "index.html");
  if (existsSync(f) && statSync(f).isDirectory()) f = join(f, "index.html");
  if (!f.startsWith(served) || !existsSync(f)) { res.writeHead(404).end("not found"); return; }
  res.writeHead(200, { "content-type": TYPES[extname(f)] || "application/octet-stream", "cache-control": "no-cache" });
  res.end(readFileSync(f));
});
const listen = () => new Promise((r) => server.listen(HTTP, "127.0.0.1", r));
const unlisten = () => new Promise((r) => { server.close(() => r()); server.closeAllConnections(); });

// ── Chrome over DevTools ─────────────────────────────────────────────────────────────────
const chrome = spawn(process.env.CHROME || "google-chrome-stable", [
  "--headless=new", `--remote-debugging-port=${DEBUG}`, `--user-data-dir=${join(WORK, "profile")}`,
  "--no-first-run", "--no-default-browser-check", "--disable-gpu", "about:blank",
], { stdio: "ignore" });
const cleanup = () => { try { chrome.kill("SIGKILL"); } catch (_) {} try { rmSync(WORK, { recursive: true, force: true }); } catch (_) {} };
process.on("exit", cleanup);

let ws, seq = 0;
const pending = new Map();
async function connect() {
  for (let i = 0; i < 50; i++) {
    try {
      const list = await (await fetch(`http://127.0.0.1:${DEBUG}/json/list`)).json();
      const page = list.find((t) => t.type === "page");
      if (page) {
        ws = new WebSocket(page.webSocketDebuggerUrl);
        await new Promise((r, j) => { ws.onopen = r; ws.onerror = j; });
        ws.onmessage = (e) => {
          const m = JSON.parse(e.data);
          if (m.id && pending.has(m.id)) { pending.get(m.id)(m); pending.delete(m.id); }
        };
        return;
      }
    } catch (_) {}
    await sleep(200);
  }
  throw new Error("Chrome did not come up on " + DEBUG);
}
const cdp = (method, params = {}) => new Promise((r) => { const id = ++seq; pending.set(id, r); ws.send(JSON.stringify({ id, method, params })); });
const js = async (expr) => {
  const m = await cdp("Runtime.evaluate", { expression: expr, awaitPromise: true, returnByValue: true });
  return m.result && m.result.result ? m.result.result.value : undefined;
};
async function until(expr, ms = 15000) {
  const end = Date.now() + ms;
  while (Date.now() < end) { const v = await js(expr); if (v) return v; await sleep(150); }
  return false;
}
const shot = async (name) => {
  const m = await cdp("Page.captureScreenshot", { format: "png" });
  writeFileSync(join(SHOTS, name + ".png"), Buffer.from(m.result.data, "base64"));
};
const go = async (url) => { await cdp("Page.navigate", { url }); await sleep(300); };
const ready = () => until("!!document.querySelector('#landing .wordmark')", 20000);
// What the worker last said, as js/app_sw.js hears it.
const SW_PROBE = `navigator.serviceWorker && navigator.serviceWorker.addEventListener("message",
  (e) => { if (e.data && e.data.type === "wado-sw") window.__sw = e.data; });`;
const hello = () => js("navigator.serviceWorker.ready.then((r) => r.active.postMessage({ type: 'hello' }))");
const sw = async (expr) => {
  const v = await until(`window.__sw && (${expr})`, 30000);
  if (!v) console.log("     worker said: " + JSON.stringify(await js("window.__sw")));
  return v;
};

try {
  const A = tree("a", { version: "0.0.3-a" });
  const B0 = tree("b-unsigned", { version: "0.0.3-b", marker: "release b", unsigned: true });
  const B = tree("b", { version: "0.0.3-b", marker: "release b" });
  const C = tree("c", { version: "0.0.3-c", marker: "release c", tamper: "icons/sc-files.png" });
  served = A;
  await listen();
  await connect();
  await cdp("Page.enable");
  await cdp("Runtime.enable");
  await cdp("Page.addScriptToEvaluateOnNewDocument", { source: SW_PROBE });
  await cdp("Emulation.setDeviceMetricsOverride", { width: 390, height: 844, deviceScaleFactor: 2, mobile: true });

  // ── 0. no release key set up (on 127.0.0.1, an origin of its own): updates are just taken ──
  const N1 = tree("n1", { version: "0.0.2-n1", nokey: true });
  const N2 = tree("n2", { version: "0.0.2-n2", marker: "release n2", nokey: true });
  served = N1;
  await go(BASE2);
  ok(await ready(), "no key: the landing renders");
  ok(await until("navigator.serviceWorker.controller !== null", 30000), "no key: a service worker controls the page");
  await hello();
  ok(await sw("__sw.pinned && !__sw.signing && __sw.version === '0.0.2-n1'"), "no key: 0.0.2-n1 is installed, signing off");
  await js("[...document.querySelectorAll('#landing .navrow')].find((b) => /Diagnostics/.test(b.textContent))?.click()");
  ok(await until("/Updates aren't signed yet — set up a release key \\(docs\\/release-signing\\.md\\)/.test(document.body.innerText)"),
    "no key: Diagnostics says \"Updates aren't signed yet — set up a release key (docs/release-signing.md)\"");
  await sleep(600);
  await shot("00-diagnostics-no-key");
  served = N2;
  await go(BASE2);
  await ready();
  await hello();
  ok(await sw("__sw.pending === '0.0.2-n2' && !__sw.refused"), "no key: an unsigned 0.0.2-n2 is taken, not refused");
  await go(BASE2);
  await ready();
  ok(await js("document.documentElement.outerHTML.includes('release n2')"), "no key: the next launch runs 0.0.2-n2");

  // The key arrives with a release (here a DEV key, honoured on 127.0.0.1): taken like any other,
  // and the new worker that carries the key switches signing on once the app is closed.
  served = A;
  await go(BASE2);
  await ready();
  await js("navigator.serviceWorker.getRegistration().then((r) => r.update())");
  await hello();
  ok(await sw("__sw.pending === '0.0.3-a'"), "no key: the release carrying the first key is taken");
  ok(await until("navigator.serviceWorker.getRegistration().then((r) => !!(r && (r.waiting || r.installing)))", 20000),
    "…and the worker carrying the key is waiting");
  await go("about:blank");
  await sleep(1500);
  await go(BASE2);
  await ready();
  await hello();
  ok(await sw("__sw.signing && __sw.version === '0.0.3-a' && !__sw.signed"), "after a relaunch signing is on, 0.0.3-a in use");
  served = B0;
  await go(BASE2);
  await ready();
  await hello();
  ok(await sw("__sw.refused && __sw.refused.kind === 'unsigned'"), "with the key in place, an unsigned update is refused");
  served = A;

  // ── 1. first install: manifest, worker, pinned (trust on first use) ──
  await go(BASE);
  ok(await ready(), "the landing renders from the network");
  ok(await until("navigator.serviceWorker.controller !== null", 30000), "a service worker controls the page");
  await hello();
  ok(await sw("__sw.pinned && __sw.version === '0.0.3-a' && __sw.signed && __sw.dev"),
    "release 0.0.3-a is pinned, signed with the DEV key");
  const man = await cdp("Page.getAppManifest");
  const mr = man.result || {};
  ok(mr.url && mr.url.endsWith("/wado/manifest.webmanifest") && !(mr.errors || []).length, "the manifest loads with no errors");
  const parsed = mr.data ? JSON.parse(mr.data) : {};
  ok((parsed.shortcuts || []).map((s) => s.url).join(" ") === "./?go=trackpad ./?go=files ./?go=shell ./?go=session",
    "it carries the four shortcuts");
  const inst = await cdp("Page.getInstallabilityErrors");
  const errs = ((inst.result || {}).installabilityErrors || []).map((e) => e.errorId);
  ok(errs.length === 0, "Chrome's install criteria are met" + (errs.length ? ` (errors: ${errs.join(", ")})` : ""));
  await shot("01-landing-installed");

  // ── 2. offline: the server is gone, the app still opens to the landing ──
  await unlisten();
  await go(BASE);
  ok(await ready(), "offline, a launch still opens the landing (from the pinned release)");
  await shot("02-landing-offline");
  await listen();

  // ── 3. an unsigned deploy is refused, and the pinned version stays ──
  served = B0;
  await go(BASE);
  await ready();
  await hello();
  ok(await sw("__sw.refused && __sw.refused.kind === 'unsigned' && __sw.refused.version === '0.0.3-b'"),
    "an unsigned 0.0.3-b is refused");
  ok(!(await js("document.documentElement.outerHTML.includes('release b')")), "…and the page is still 0.0.3-a");
  await go(BASE);
  await ready();
  ok(!(await js("document.documentElement.outerHTML.includes('release b')")), "…also after another launch");
  await js("[...document.querySelectorAll('#landing .navrow')].find((b) => /Diagnostics/.test(b.textContent))?.click()");
  ok(await until("/An unsigned update was refused/.test(document.body.innerText)"), "Diagnostics says: an unsigned update was refused");
  await sleep(600);
  await shot("03-diagnostics-refused");

  // ── 4. the same release, DEV-signed: downloaded, then used from the next launch ──
  served = B;
  await go(BASE);
  await ready();
  await hello();
  ok(await sw("__sw.pending === '0.0.3-b' && !__sw.refused"), "a signed 0.0.3-b is verified and waits");
  ok(await until("/Update ready — reopen to use it/.test(document.querySelector('.toasts').innerText)"),
    "the toast says: Update ready — reopen to use it");
  ok(!(await js("document.documentElement.outerHTML.includes('release b')")), "…and this page is still 0.0.3-a (never mid-session)");
  await shot("04-update-ready-toast");
  await go(BASE);
  await ready();
  ok(await js("document.documentElement.outerHTML.includes('release b')"), "the next launch runs 0.0.3-b");
  await hello();
  ok(await sw("__sw.version === '0.0.3-b' && __sw.signed && !__sw.pending"), "…and the worker says 0.0.3-b is in use");
  await js("[...document.querySelectorAll('#landing .navrow')].find((b) => /Diagnostics/.test(b.textContent))?.click()");
  ok(await until("/wado 0\\.0\\.3-b · signed ✓ \\(dev key\\) · updated/.test(document.body.innerText)"),
    "Diagnostics: wado 0.0.3-b · signed ✓ (dev key) · updated <date>");
  await sleep(600);
  await shot("05-diagnostics-signed");

  // ── 5. signed, but a file was changed after signing: refused ──
  served = C;
  await go(BASE);
  await ready();
  await hello();
  ok(await sw("__sw.refused && __sw.refused.kind === 'hash' && __sw.version === '0.0.3-b'"),
    "a signed 0.0.3-c with a changed file is refused (hash), 0.0.3-b stays");
  served = B;

  // ── 6. shortcuts ──
  const settings = (relay) => JSON.stringify({
    conn_mode: "relay", relay_url: relay, remote_id: "123456789", profile: 0,
    profiles: [{ name: "Desk", host: "", conn_mode: "relay", relay_url: relay, remote_id: "123456789" }],
  });
  const view = () => js("document.querySelector('#computer') ? 'computer' : 'home'");
  const toast = () => js("document.querySelector('.toasts').innerText");

  // No computer saved: home, with a word.
  await js("localStorage.clear()");
  await go(BASE + "?go=trackpad");
  await ready();
  ok(await until("/Add a computer first/.test(document.querySelector('.toasts').innerText)"), "no computer yet: home, \"Add a computer first\"");
  ok((await view()) === "home" && !(await js("location.search.includes('go=')")), "…on home, and ?go= is gone from the address");

  // A computer that answers (played by the fixture hook): each shortcut opens its door.
  // The relay is a blackhole address, so the real link stays "connecting" and never says offline.
  const FIX = `W=window.__wado;W.fixture({type:"phase",stage:2});W.fixture({type:"hostConfig",state:{}});`;
  for (const [name, spy, door] of [
    ["trackpad", "W.padOpen=()=>{window.__opened='trackpad'}", "window.__opened==='trackpad'"],
    ["files", "W.filesOpen=()=>{window.__opened='files'}", "window.__opened==='files'"],
    ["shell", "W.shellNew=()=>{window.__opened='shell'};W.ptyShow=W.shellNew", "window.__opened==='shell'"],
    ["session", "W.start=()=>{window.__opened='started'}", "document.activeElement && document.activeElement.matches('#computer .act.primary')"],
  ]) {
    await js(`localStorage.clear(); localStorage.setItem("wado.settings", ${JSON.stringify(settings("http://10.255.255.1:9"))})`);
    await go(BASE + `?fixture&go=${name}`);
    ok(await until("!!document.querySelector('#computer')", 10000), `?go=${name}: the last used computer's page opens`);
    await js(`(() => { const W = window.__wado; ${spy}; })()`);
    await sleep(400);
    await js(FIX);
    ok(await until(door, 8000), `?go=${name}: ` + (name === "session" ? "lands on its session door, nothing started" : "opens " + name));
    if (name === "session") ok(!(await js("window.__opened === 'started'")), "…and starts no session by itself");
    await shot(`06-go-${name}`);
  }

  // Not paired yet: home.
  await js(`localStorage.clear(); localStorage.setItem("wado.settings", ${JSON.stringify(settings("http://10.255.255.1:9"))})`);
  await go(BASE + "?fixture&go=files");
  await until("!!document.querySelector('#computer')", 10000);
  await sleep(400);
  await js(`window.__wado.fixture({type:"unpaired",reason:"This computer doesn't know this device yet"})`);
  ok(await until("/hasn't paired with this phone/.test(document.querySelector('.toasts').innerText)"), "unpaired: \"…hasn't paired with this phone yet\"");
  ok(await until("!document.querySelector('#computer')", 5000), "…and back on home");
  await shot("07-go-unpaired-home");

  // Offline (the relay says no daemon is there): home.
  await js(`localStorage.clear(); localStorage.setItem("wado.settings", ${JSON.stringify(settings("http://10.255.255.1:9"))})`);
  await go(BASE + "?fixture&go=shell");
  await until("!!document.querySelector('#computer')", 10000);
  await sleep(400);
  await js(`window.__wado.fixture({type:"link",state:"computer_offline",path:"",rtt_ms:null,net:"",nat:"",turn:false,why:"no computer"})`);
  ok(await until("/Desk is offline/.test(document.querySelector('.toasts').innerText)"), "offline: \"Desk is offline — here are your computers.\"");
  ok(await until("!document.querySelector('#computer')", 5000), "…and back on home");
  await shot("08-go-offline-home");
  console.log("toast: " + (await toast()));

  // ── 7. iOS Safari: the install hint, once ──
  await cdp("Emulation.setUserAgentOverride", {
    userAgent: "Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Mobile/15E148 Safari/604.1",
  });
  await js("localStorage.clear()");
  await go(BASE);
  await ready();
  ok(await until("!!document.querySelector('.installhint')"), "iOS Safari: the Add to Home Screen hint shows");
  await sleep(700);
  await shot("09-ios-install-hint");
  await go(BASE);
  await ready();
  await sleep(1500);
  ok(!(await js("!!document.querySelector('.installhint')")), "…and only once");
} catch (e) {
  console.error(e);
  failed++;
} finally {
  try { await unlisten(); } catch (_) {}
  console.log(`\nscreenshots: ${SHOTS}`);
  console.log(failed ? `${failed} FAILED` : "all passed");
  cleanup();
  process.exit(failed ? 1 : 0);
}
