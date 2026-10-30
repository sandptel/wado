// How long a network change costs, to picture — measured, not reasoned (Decision Log
// 2026-10-27: "< 2 s to picture behind a Reconnecting… overlay; an ICE restart only if that
// misses"). The companion of reconnect-check.mjs, which pins the routing; this one times it.
//
//   cargo build --release -p wado -p wado-relay
//   node scripts/reconnect-time.mjs [runs]          (needs a GPU and Chrome; sandbox off)
//   WADO_BIN=target/baseline node scripts/reconnect-time.mjs   another pair of binaries
//
// Against the real binaries — a local relay and one release daemon with its own config — and the
// client's **own bundle**: every js/ file bridge.rs concatenates, loaded into a headless Chrome
// page in the same order, so the recovery that is timed is the one that ships (netwatch.js →
// relayKick → the envelope → relay.js's rejoin → _relayNegotiate), not a re-enactment of it.
//
// Three numbers per run, each from the event to the first frame the new peer connection decodes:
//   cold      Start pressed on a warm link → picture (the reference)
//   media     a fresh peer connection over the still-open relay socket (`reconnectWebRTC`),
//             session untouched — the plan's path
//   network   the browser says the network changed (`online`): netwatch.js redials the link
//             (takeover of its own seat), the envelope is redone, the session is rejoined and a
//             fresh peer connection is built — what a Wi-Fi → mobile handoff runs
// What none of them include: the time a real handoff takes to be *noticed* (the browser's own
// `online`/`change` event, or 1.5 s of `disconnected` plus the browser's consent timeout). That
// part is the browser's and the radio's, and is reported separately when it can be measured.
//
// Kills only what it started, by pid.

import { spawn, execFileSync } from "node:child_process";
import { mkdtempSync, mkdirSync, readFileSync } from "node:fs";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { join } from "node:path";

const ROOT = new URL("..", import.meta.url).pathname;
const BIN = join(ROOT, process.env.WADO_BIN || "target/release");
const RUNS = Number(process.argv[2] || 5);
const PORT = 4985;
const HTTP = 4986;
const CDP = 9371;
const RELAY = `http://127.0.0.1:${PORT}`;
const RID = "999000555";
const T = mkdtempSync(join(tmpdir(), "wado-reconnect-"));
const CFG = join(T, "cfg");
const RUN = join(T, "run");
mkdirSync(join(CFG, "wado"), { recursive: true });
mkdirSync(RUN, { recursive: true, mode: 0o700 });
const ENV = { XDG_CONFIG_HOME: CFG, XDG_RUNTIME_DIR: RUN };
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const procs = new Set();

function start(bin, args, env = {}) {
  const p = spawn(bin, args, { env: { ...process.env, ...ENV, ...env }, stdio: ["ignore", "pipe", "pipe"] });
  p.out = "";
  const take = (d) => { p.out += d.toString().replace(/\x1b\[[0-9;]*m/g, ""); };
  p.stdout.on("data", take);
  p.stderr.on("data", take);
  procs.add(p);
  p.on("exit", () => procs.delete(p));
  return p;
}
function cleanup() { for (const p of procs) try { p.kill("SIGTERM"); } catch {} }
for (const sig of ["SIGTERM", "SIGINT", "SIGHUP"]) process.on(sig, () => { cleanup(); process.exit(1); });
async function waitFor(p, re, ms = 20000) {
  const t0 = Date.now();
  while (Date.now() - t0 < ms) { if (re.test(p.out)) return true; await sleep(100); }
  return false;
}

// ── The client bundle, as bridge.rs assembles it ─────────────────────────────
// Each js/ file becomes its own classic script (they share one global lexical scope, as they do
// inside the one eval), so a file that throws at load is reported by name instead of silently
// taking every later file with it. Literal pieces between includes are kept with their neighbours.
function bundle() {
  const src = readFileSync(join(ROOT, "crates/client/src/bridge.rs"), "utf8");
  const body = src.slice(src.indexOf("pub const JS: &str = concat!("), src.indexOf(");\n", src.indexOf("pub const JS")));
  const unesc = (s) => s.replace(/\\(.)/g, (_, c) => (c === "n" ? "\n" : c));
  const parts = [];
  let inline = "";
  for (const m of body.matchAll(/include_str!\("([^"]+)"\)|"((?:[^"\\]|\\.)*)"/g)) {
    if (m[1] && m[1].startsWith("js/") && m[1].endsWith(".js")) {
      if (inline.trim()) parts.push({ name: "inline", code: inline });
      inline = "";
      let code = readFileSync(join(ROOT, "crates/client/src", m[1]), "utf8");
      // The eval is an async function body; a classic script cannot await at top level.
      if (m[1] === "js/lifecycle.js") code = code.replace(/\nawait new Promise\(\(\) => \{\}\);\s*$/, "\n");
      parts.push({ name: m[1], code });
    } else if (m[1]) inline += readFileSync(join(ROOT, "crates/client/src", m[1]), "utf8");
    else inline += unesc(m[2]);
  }
  if (inline.trim()) parts.push({ name: "inline", code: inline });
  return parts;
}

const PARTS = bundle();
const PAGE = `<!doctype html><html><body>
<video id="wado-video" muted autoplay playsinline></video>
<script>
  window.__ev = []; window.__errs = [];
  window.dioxus = { send: (m) => window.__ev.push([performance.now(), m]) };
  window.addEventListener("error", (e) => window.__errs.push((e.filename || "") + ": " + e.message));
</script>
${PARTS.map((_, i) => `<script src="/b/${i}.js"></script>`).join("\n")}
</body></html>`;
const http = createServer((req, res) => {
  const m = /^\/b\/(\d+)\.js$/.exec(req.url);
  if (m) { res.writeHead(200, { "content-type": "text/javascript" }); return res.end(PARTS[+m[1]].code); }
  res.writeHead(200, { "content-type": "text/html" });
  res.end(PAGE);
});

// ── Chrome over CDP ──────────────────────────────────────────────────────────
class Page {
  constructor() { this.id = 0; this.pend = new Map(); }
  async open() {
    start("google-chrome-stable", ["--headless=new", "--remote-debugging-port=" + CDP, "--user-data-dir=" + join(T, "chrome"),
      "--autoplay-policy=no-user-gesture-required", "--no-first-run", "about:blank"]);
    let list;
    for (let i = 0; i < 60 && !list; i++) { await sleep(200); try { list = await (await fetch(`http://127.0.0.1:${CDP}/json/list`)).json(); } catch {} }
    this.ws = new WebSocket(list.find((t) => t.type === "page").webSocketDebuggerUrl);
    await new Promise((r) => (this.ws.onopen = r));
    this.ws.onmessage = (e) => { const m = JSON.parse(e.data); if (m.id && this.pend.has(m.id)) { this.pend.get(m.id)(m); this.pend.delete(m.id); } };
  }
  cdp(method, params = {}) { return new Promise((r) => { const i = ++this.id; this.pend.set(i, r); this.ws.send(JSON.stringify({ id: i, method, params })); }); }
  async ev(expr) {
    const r = await this.cdp("Runtime.evaluate", { expression: expr, returnByValue: true, awaitPromise: true });
    if (r.result?.exceptionDetails) throw new Error(JSON.stringify(r.result.exceptionDetails).slice(0, 400));
    return r.result?.result?.value;
  }
}

// In the page: when did the current peer connection decode its first frame?
const WATCH = `(() => {
  if (window.__watch) return;
  window.__watch = { pc: null, firstFrame: 0, connected: 0 };
  setInterval(async () => {
    const pc = window.__wado.pc, w = window.__watch;
    if (!pc) return;
    if (pc !== w.pc) { w.pc = pc; w.firstFrame = 0; w.connected = 0; }
    if (!w.connected && pc.connectionState === "connected") w.connected = performance.now();
    if (w.firstFrame) return;
    try {
      (await pc.getStats()).forEach((r) => {
        if (r.type === "inbound-rtp" && r.kind === "video" && r.framesDecoded > 0 && !w.firstFrame && pc === w.pc) w.firstFrame = performance.now();
      });
    } catch (_) {}
  }, 5);
})()`;

// Wait for a fresh peer connection (not `old`) to decode; the times, relative to `t0`.
async function picture(page, t0, old, ms = 15000) {
  const end = Date.now() + ms;
  while (Date.now() < end) {
    const w = await page.ev(`(() => { const w = window.__watch; return { same: w.pc === window.__old, f: w.firstFrame, c: w.connected }; })()`);
    if (!w.same && w.f) return { frame: Math.round(w.f - t0), connected: w.c ? Math.round(w.c - t0) : null };
    await sleep(20);
  }
  return null;
}

const stats = (xs) => {
  const s = xs.filter((x) => x != null).sort((a, b) => a - b);
  if (!s.length) return "no samples";
  const q = (p) => s[Math.min(s.length - 1, Math.floor(p * s.length))];
  return `median ${q(0.5)} ms · min ${s[0]} · max ${s[s.length - 1]} · n=${s.length}`;
};

let failures = 0;
try {
  await new Promise((r) => http.listen(HTTP, "127.0.0.1", r));
  start(join(BIN, "wado-relay"), ["--bind", "127.0.0.1:" + PORT]);
  await sleep(400);
  const d = start(join(BIN, "wado"), ["daemon"], { WADO_RELAY_URL: `ws://127.0.0.1:${PORT}`, WADO_REMOTE_ID: RID, WADO_INSTANCE: "1", WADO_UDP_SLICE: "14" });
  if (!(await waitFor(d, /registered — Remote ID/))) throw new Error("the daemon never registered:\n" + d.out.slice(-800));
  const qr = execFileSync(join(BIN, "wado"), ["qr", "--relay", RELAY, "--id", RID], { env: { ...process.env, ...ENV } })
    .toString().replace(/\x1b\[[0-9;]*m/g, "");
  const q = new URLSearchParams(qr.match(/open: \S+\?(\S+)/)[1]);

  const page = new Page();
  await page.open();
  await page.cdp("Page.navigate", { url: `http://127.0.0.1:${HTTP}/` });
  await sleep(1500);
  const errs = await page.ev("window.__errs");
  if (errs.length) console.log("  (bundle load errors, not fatal unless they are in the link files)\n    " + errs.join("\n    "));
  await page.ev(`(() => {
    localStorage.setItem("wado.pair", ${JSON.stringify(q.get("pair"))});
    localStorage.setItem("wado.hostpins", JSON.stringify({ ${JSON.stringify(RID)}: { pin: ${JSON.stringify(q.get("hk"))}, from: "qr" } }));
    const W = window.__wado;
    window.__rlog = [];
    const r = W.rlog; W.rlog = (l) => { window.__rlog.push([performance.now(), l]); return r(l); };
    return true;
  })()`);
  await page.ev(WATCH);

  // ── cold start ──
  const config = { width: 1280, height: 720, fps: 60, scale: 1, quality: { custom: { bitrate_kbps: 6000 } } };
  const t0 = await page.ev(`(() => { window.__old = null; const t = performance.now();
    window.__wado.relayConnect(${JSON.stringify(RELAY)}, ${JSON.stringify(RID)}, ${JSON.stringify(config)}); return t; })()`);
  const cold = await picture(page, t0, null, 40000);
  if (!cold) throw new Error("no picture on the cold start:\n" + JSON.stringify(await page.ev("window.__rlog.slice(-20)")) + "\n" + d.out.slice(-1200));
  console.log(`cold start: picture ${cold.frame} ms (connected ${cold.connected} ms)`);
  await sleep(2000);

  const media = [], network = [], mediaC = [], networkC = [], clientSays = [];
  for (let i = 0; i < RUNS; i++) {
    // ── media path only: a fresh peer connection over the open relay socket ──
    const tm = await page.ev(`(() => { window.__old = window.__wado.pc; const t = performance.now(); window.__wado.reconnectWebRTC(); return t; })()`);
    const a = await picture(page, tm, null);
    media.push(a && a.frame); mediaC.push(a && a.connected);
    console.log(`  run ${i + 1} media:   ${a ? `picture ${a.frame} ms, connected ${a.connected} ms` : "NO PICTURE in 15 s"}`);
    if (!a) failures++;
    await sleep(2500);

    // ── a network change, as the browser reports one ──
    const tn = await page.ev(`(() => { window.__old = window.__wado.pc; window.__wado.lastRecovery = null; const t = performance.now(); window.dispatchEvent(new Event("online")); return t; })()`);
    const b = await picture(page, tn, null);
    await sleep(300);
    const said = await page.ev("window.__wado.lastRecovery && window.__wado.lastRecovery.ms");
    network.push(b && b.frame); networkC.push(b && b.connected); clientSays.push(said);
    console.log(`  run ${i + 1} network: ${b ? `picture ${b.frame} ms, connected ${b.connected} ms, client's own "reconnected in" ${said} ms` : "NO PICTURE in 15 s"}`);
    if (!b) {
      failures++;
      console.log(JSON.stringify((await page.ev("window.__rlog.slice(-25)")).map(([t, l]) => Math.round(t - tn) + " " + l), null, 1));
    }
    await sleep(2500);
  }
  // One breakdown of the last network run, from the client's own log lines (every `rlog`
  // reaches the UI as a `log` event, whichever file wrote it).
  const last = await page.ev(`window.__ev.filter(([, m]) => m.type === "log").map(([t, m]) => [t, m.line.split("|").slice(2).join("|")])`);
  console.log("\nlast network run, from the client's log (ms after `online`):");
  const base = last.filter(([, l]) => /network: network back/.test(l)).pop();
  if (base) for (const [t, l] of last.filter(([t]) => t >= base[0] - 400).slice(0, 40)) console.log(`  ${String(Math.round(t - base[0])).padStart(6)}  ${l.slice(0, 150)}`);

  console.log(`\nmedia path (fresh peer connection, relay socket kept): picture ${stats(media)}`);
  console.log(`                                                     connected ${stats(mediaC)}`);
  console.log(`network change (redial + envelope + rejoin + fresh pc): picture ${stats(network)}`);
  console.log(`                                                     connected ${stats(networkC)}`);
  console.log(`client's own recovery figure (netwatch.js):            ${stats(clientSays)}`);
  const med = network.filter((x) => x != null).sort((a, b) => a - b)[Math.floor(network.length / 2)];
  console.log(med != null && med < 2000 ? "\nnetwork change: under the 2 s target" : "\nnetwork change: MISSES the 2 s target");
} catch (e) {
  failures++;
  console.log("FAIL " + (e.stack || e));
} finally {
  cleanup();
  http.close();
}
process.exit(failures ? 1 : 0);
