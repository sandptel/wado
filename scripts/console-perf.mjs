// How the Pocket Console feels on a slow phone: a CPU-throttled trace of the console while the
// picture plays, and of real touch taps on its controls (`?fixture`, no computer behind the page).
//
//   dx build -p wado-client --platform web
//   node scripts/console-perf.mjs [out.json]          (needs Chrome; run with the sandbox off)
//   RATES=4,6 FORMS=390x844,844x390 BUDGET=150 node scripts/console-perf.mjs
//
// Per form and throttling rate it reports:
//   idle    — 4 s of the picture playing, nobody touching: main-thread busy ms/s, style recalcs,
//             layouts, paints and DOM mutations per second, long tasks (a re-render or layout
//             storm shows up here as work that should be zero)
//   taps    — a real touch (touchStart/touchEnd) on deck keys, rail buttons, the faces, the
//             window strip and the Full screen toggle: input delay, processing and input-to-paint
//             (Event Timing), plus how soon the pressed state is visible
// With BUDGET set it fails (exit 1) when any tap's input-to-paint at 4× exceeds it.

import { spawn } from "node:child_process";
import { mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const ROOT = new URL("..", import.meta.url).pathname;
const PUB = process.env.WADO_PUB || join(ROOT, "target/dx/wado-client/debug/web/public");
const OUTF = process.argv[2] || "";
const RATES = (process.env.RATES || "4,6").split(",").map(Number);
const FORMS = (process.env.FORMS || "390x844,844x390").split(",").map((f) => f.split("x").map(Number));
const BUDGET = +(process.env.BUDGET || 0);
const T = mkdtempSync(join(tmpdir(), "wado-perf-"));
const HTTP = 8777, CDP = 9377;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const procs = [];
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
let id = 0; const pend = new Map(); const listeners = new Map();
ws.onmessage = (e) => {
  const m = JSON.parse(e.data);
  if (m.id && pend.has(m.id)) { pend.get(m.id)(m); pend.delete(m.id); }
  else if (m.method && listeners.has(m.method)) for (const f of listeners.get(m.method)) f(m.params);
};
const on = (method, f) => { if (!listeners.has(method)) listeners.set(method, []); listeners.get(method).push(f); };
const cdp = (method, params = {}) => new Promise((r) => { const i = ++id; pend.set(i, r); ws.send(JSON.stringify({ id: i, method, params })); });
const js = async (expr) => {
  const r = await cdp("Runtime.evaluate", { expression: expr, returnByValue: true, awaitPromise: true });
  if (r.result?.exceptionDetails) throw new Error(r.result.exceptionDetails.exception?.description || "evaluate failed");
  return r.result?.result?.value;
};
const until = async (expr, ms = 8000) => { const t = Date.now(); while (Date.now() - t < ms) { if (await js(expr).catch(() => false)) return true; await sleep(120); } return false; };

await cdp("Runtime.enable");
await cdp("Page.enable");
const BASE = `http://127.0.0.1:${HTTP}/`;

// The session's picture: a desktop on a canvas at 30 fps, streamed into #wado-video.
const PICTURE = `(() => {
  const v = document.getElementById("wado-video");
  if (!v) return false;
  const W0 = 780, H0 = 1688;
  const c = document.createElement("canvas"); c.width = W0; c.height = H0;
  const g = c.getContext("2d"); let n = 0;
  const draw = () => {
    n++;
    g.fillStyle = "#2b3a4a"; g.fillRect(0, 0, W0, H0);
    g.fillStyle = "#f4f1ea"; g.fillRect(W0 * .04, H0 * .05, W0 * .92, H0 * .86);
    g.fillStyle = "#d9622b"; g.fillRect((n * 7) % W0, H0 * .5, 60, 60);
  };
  draw(); setInterval(draw, 33);
  v.srcObject = c.captureStream(30);
  v.muted = true; v.play().catch(() => {});
  return true;
})()`;

const WINDOWS = { type: "windows", workspace: 1, windows: [
  { id: 1, title: "Repotting a monstera", app_id: "firefox", focused: true, workspace: 1 },
  { id: 2, title: "~/src/wado — cargo test", app_id: "foot", focused: false, workspace: 1 },
  { id: 3, title: "Downloads", app_id: "org.gnome.Nautilus", focused: false, workspace: 2 },
] };

async function open(w, h) {
  await cdp("Emulation.setCPUThrottlingRate", { rate: 1 });
  await cdp("Emulation.setDeviceMetricsOverride", { width: w, height: h, deviceScaleFactor: 3, mobile: true });
  await cdp("Emulation.setTouchEmulationEnabled", { enabled: true, maxTouchPoints: 5 });
  const settings = JSON.stringify({ conn_mode: "relay", relay_url: "http://10.255.255.1:9", remote_id: "123456789", profile: 0,
    theme: "default-dark",
    profiles: [{ name: "Framework", host: "", conn_mode: "relay", relay_url: "http://10.255.255.1:9", remote_id: "123456789" }] });
  await cdp("Page.navigate", { url: BASE });
  await sleep(400);
  await js(`localStorage.clear(); localStorage.setItem("wado.settings", ${JSON.stringify(settings)});
    localStorage.setItem("wado.console", JSON.stringify({open: "console", face: "keys"})); true`);
  await cdp("Page.navigate", { url: BASE + "?fixture" });
  await until("!!window.__wado && !!window.__wado.fixture", 10000);
  await js(`(() => { const W = window.__wado;
    W.fixture({ type: "sessionOn" });
    W.fixture(${JSON.stringify(WINDOWS)});
    W.fixture({ type: "encoder", mode: "hardware", pipeline: "vaapi-dmabuf" });
    W.fixture({ type: "link", state: "direct", path: "lan", rtt_ms: 23, net: "wifi" });
    return true; })()`);
  await until("document.documentElement.dataset.arr === 'console' && !!document.getElementById('wado-video')", 8000);
  await js(PICTURE);
  await until("document.getElementById('wado-video').videoWidth > 0", 6000);
  await sleep(1500);
  // A session's telemetry, as the bridge sends it while a picture plays: stats once a second,
  // the link sample, and the host poll — the updates a re-render storm would ride on.
  await js(`(() => { const W = window.__wado; let n = 0;
    window.__telemetry = setInterval(() => { n++;
      W.fixture({ type: "stats", fps: 59 + (n % 2), ping: 20 + (n % 5) });
      W.fixture({ type: "link", state: "direct", path: "lan", rtt_ms: 20 + (n % 7), net: "wifi" });
    }, 1000); return true; })()`);
  await js(`(() => {
    window.__perf = { events: [], long: [], muts: 0 };
    new PerformanceObserver((l) => { for (const e of l.getEntries()) window.__perf.events.push({ name: e.name, start: e.startTime,
      delay: e.processingStart - e.startTime, proc: e.processingEnd - e.processingStart, dur: e.duration,
      target: e.target ? (e.target.getAttribute("aria-label") || e.target.className || e.target.tagName) : "" }); })
      .observe({ type: "event", durationThreshold: 16, buffered: false });
    new PerformanceObserver((l) => { for (const e of l.getEntries()) window.__perf.long.push({ start: e.startTime, dur: e.duration }); })
      .observe({ type: "longtask", buffered: false });
    new MutationObserver((ms) => { window.__perf.muts += ms.length; })
      .observe(document.body, { subtree: true, childList: true, attributes: true, characterData: true });
    return true; })()`);
}

// One trace window: Chrome's own timeline events, summed by kind.
async function trace(fn) {
  const events = [];
  let done;
  const finished = new Promise((r) => (done = r));
  const onData = (p) => events.push(...p.value);
  on("Tracing.dataCollected", onData);
  on("Tracing.tracingComplete", () => done());
  await cdp("Tracing.start", { traceConfig: { includedCategories: ["devtools.timeline", "disabled-by-default-devtools.timeline", "blink.user_timing"] }, transferMode: "ReportEvents" });
  const t0 = Date.now();
  await fn();
  const secs = (Date.now() - t0) / 1000;
  await cdp("Tracing.end");
  await finished;
  listeners.get("Tracing.dataCollected").length = 0;
  listeners.get("Tracing.tracingComplete").length = 0;
  // The renderer's main thread: the one that runs RunTask with the most total time.
  const mainTid = (() => {
    const by = new Map();
    for (const e of events) if (e.name === "RunTask" && e.dur) by.set(e.tid, (by.get(e.tid) || 0) + e.dur);
    return [...by.entries()].sort((a, b) => b[1] - a[1])[0]?.[0];
  })();
  const sum = (name) => events.filter((e) => e.name === name && e.tid === mainTid);
  const ms = (arr) => arr.reduce((a, e) => a + (e.dur || 0), 0) / 1000;
  const tasks = sum("RunTask");
  const out = {
    secs: +secs.toFixed(2),
    busyMsPerS: +(ms(tasks) / secs).toFixed(1),
    longTasks: tasks.filter((e) => e.dur > 50000).length,
    styleRecalcPerS: +(sum("UpdateLayoutTree").length / secs).toFixed(1),
    styleMsPerS: +(ms(sum("UpdateLayoutTree")) / secs).toFixed(2),
    layoutPerS: +(sum("Layout").length / secs).toFixed(1),
    layoutMsPerS: +(ms(sum("Layout")) / secs).toFixed(2),
    paintPerS: +(sum("Paint").length / secs).toFixed(1),
    paintMsPerS: +(ms(sum("Paint")) / secs).toFixed(2),
    scriptMsPerS: +((ms(sum("FunctionCall")) + ms(sum("EvaluateScript")) + ms(sum("TimerFire")) + ms(sum("FireAnimationFrame"))) / secs).toFixed(2),
  };
  return out;
}

// A real touch: touchStart, a short hold, touchEnd — Chrome turns it into pointer events and a
// click through its own gesture detection, exactly as a finger would.
async function tap(sel) {
  const r = await js(`(() => { const el = document.querySelector(${JSON.stringify(sel)});
    if (!el || el.closest("[inert]")) return null; const b = el.getBoundingClientRect();
    if (!b.width) return null; return { x: b.left + b.width / 2, y: b.top + b.height / 2 }; })()`);
  if (!r) return null;
  await js(`(() => { const el = document.querySelector(${JSON.stringify(sel)});
    window.__press = { t0: performance.now(), seen: 0 };
    const look = () => { const s = getComputedStyle(el); const k = parseFloat(s.scale) || 1;
      const tr = s.transform; const m = tr && tr !== "none" ? new DOMMatrix(tr).a : 1;
      if ((k < 0.995 || m < 0.995 || el.matches(":active,[data-press]")) && !window.__press.seen) window.__press.seen = performance.now() - window.__press.t0;
      if (!window.__press.seen && performance.now() - window.__press.t0 < 400) requestAnimationFrame(look); };
    requestAnimationFrame(look); return true; })()`);
  await cdp("Input.dispatchTouchEvent", { type: "touchStart", touchPoints: [{ x: r.x, y: r.y, id: 1 }] });
  await sleep(70);
  await cdp("Input.dispatchTouchEvent", { type: "touchEnd", touchPoints: [] });
  await sleep(450);
  return js("window.__press.seen");
}

const TAPS = [
  ["deck key (Esc)", ".cdeck [data-dkey='esc']"],
  ["deck mod (Ctrl)", ".cdeck [data-dmod='ctrl']"],
  ["face: Quick", ".cfaces [data-face='quick']"],
  ["face: Keys", ".cfaces [data-face='keys']"],
  ["rail: rotate", ".rail [data-rotate]"],
  ["strip: Minimise", ".cstrip .cwin"],
  ["workspace 2", "#wado-bar .wsnum:nth-of-type(2)"],
  ["Full screen", "[data-arrange='toggle']"],
  ["Leave full screen", "[data-arrange='toggle']"],
];

const report = { at: new Date().toISOString(), forms: [] };
let over = 0;
for (const [w, h] of FORMS) {
  for (const rate of RATES) {
    await open(w, h);
    // Window actions and rotate go nowhere in a fixture; keep them from erroring.
    await js(`window.__wado.windowAction = () => {}; true`);
    await cdp("Emulation.setCPUThrottlingRate", { rate });
    await sleep(500);
    await js(`window.__perf.muts = 0; window.__perf.long = []; true`);
    const idle = await trace(() => sleep(4000));
    const mutsIdle = await js("window.__perf.muts");
    if (process.env.PROFILE) {
      await cdp("Profiler.enable"); await cdp("Profiler.setSamplingInterval", { interval: 200 });
      await cdp("Profiler.start"); await sleep(4000);
      const { result } = await cdp("Profiler.stop");
      const p = result.profile, self = new Map(), byId = new Map(p.nodes.map((n) => [n.id, n]));
      const dt = p.timeDeltas; const counts = new Map();
      p.samples.forEach((s, i) => counts.set(s, (counts.get(s) || 0) + (dt[i] || 0)));
      for (const [nid, us] of counts) { const n = byId.get(nid); const cf = n.callFrame;
        const k = (cf.functionName || "(anon)") + " " + (cf.url.split("/").pop() || "") + ":" + cf.lineNumber;
        self.set(k, (self.get(k) || 0) + us); }
      console.log("top self time (ms over 4 s):");
      for (const [k, us] of [...self.entries()].sort((a, b) => b[1] - a[1]).slice(0, 25)) console.log("   ", (us / 1000).toFixed(1).padStart(7), k);
    }
    idle.domMutationsPerS = +(mutsIdle / idle.secs).toFixed(1);
    const taps = [];
    for (const [name, sel] of TAPS) {
      await js(`window.__perf.events = []; true`);
      const press = await tap(sel);
      if (press === null) { taps.push({ name, skipped: true }); continue; }
      await sleep(250);
      const ev = await js("window.__perf.events");
      const worst = ev.reduce((a, e) => (e.dur > (a?.dur || 0) ? e : a), null);
      const t = { name, pressVisibleMs: press ? +press.toFixed(0) : null,
        inputToPaintMs: worst ? Math.round(worst.dur) : "<16", worstEvent: worst ? worst.name : "",
        inputDelayMs: worst ? Math.round(worst.delay) : 0, processingMs: worst ? Math.round(worst.proc) : 0 };
      taps.push(t);
      if (BUDGET && rate === 4 && worst && worst.dur > BUDGET) over++;
      // Full screen settles before the next tap.
      if (/Full screen/.test(name)) await sleep(900);
    }
    const long = await js("window.__perf.long");
    const row = { form: `${w}x${h}`, cpu: `${rate}x`, idle, taps, longTasksDuringTaps: long.length, longestTaskMs: long.length ? Math.round(Math.max(...long.map((l) => l.dur))) : 0 };
    report.forms.push(row);
    console.log(`\n== ${row.form} @ ${row.cpu} CPU ==`);
    console.log("idle (picture playing):", JSON.stringify(idle));
    for (const t of taps) console.log("  tap", t.name.padEnd(20), t.skipped ? "(not on screen)" : `press visible ${t.pressVisibleMs ?? "never"} ms · input→paint ${t.inputToPaintMs} ms (${t.worstEvent}, delay ${t.inputDelayMs}, handlers ${t.processingMs})`);
    console.log(`  long tasks during taps: ${row.longTasksDuringTaps}, longest ${row.longestTaskMs} ms`);
  }
}
if (OUTF) writeFileSync(OUTF, JSON.stringify(report, null, 2));
cleanup();
if (BUDGET) console.log(over ? `\n${over} taps over the ${BUDGET} ms budget at 4×` : `\nevery tap under ${BUDGET} ms at 4×`);
process.exit(BUDGET && over ? 1 : 0);
