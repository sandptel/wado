// Main-thread cost of the client while a session streams, in a phone-emulated Chrome.
//
//   node scripts/perf-probe.mjs [seconds]     (after dx build + release build; sandbox off)
//
// Prints, per phase (idle stream, control centre open): main-thread busy %, script %, style
// recalcs and layouts per second, and which bridge messages (JS → Rust) fire how often — each
// one is a WASM re-render, and a phone decoding video has little main thread to spare.
import { spawn } from "node:child_process";
import { mkdtempSync, mkdirSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
const ROOT = new URL("..", import.meta.url).pathname;
const PUB = ROOT + "target/dx/wado-client/debug/web/public";
const PORT = 4997, RID = "999000777", HTTP = 8767, DBG = 9351;
const SECS = Number(process.argv[2] || 10);
const T = mkdtempSync(join(tmpdir(), "wado-perf-"));
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const procs = [];
const start = (cmd, args, env = {}) => { const p = spawn(cmd, args, { env: { ...process.env, ...env }, stdio: "ignore" }); procs.push(p); return p; };
mkdirSync(join(T, "cfg/wado"), { recursive: true });
writeFileSync(join(T, "cfg/wado/trusted_clients"), "Perf-key\tPhone\n");
start(ROOT + "target/release/wado-relay", ["--bind", "127.0.0.1:" + PORT]);
await sleep(400);
start(ROOT + "target/release/wado", [], { WADO_RELAY_URL: "ws://127.0.0.1:" + PORT, WADO_REMOTE_ID: RID, WADO_INSTANCE: "perf", XDG_CONFIG_HOME: join(T, "cfg"), WADO_UDP_SLICE: "3" });
start("python3", ["-m", "http.server", String(HTTP), "--bind", "127.0.0.1", "-d", PUB]);
start("google-chrome-stable", ["--headless=new", "--remote-debugging-port=" + DBG, "--user-data-dir=" + join(T, "chrome"),
  "--use-fake-ui-for-media-stream", "--autoplay-policy=no-user-gesture-required", "--no-first-run", "about:blank"]);
let list; for (let i = 0; i < 50 && !list; i++) { await sleep(200); try { list = await (await fetch(`http://127.0.0.1:${DBG}/json/list`)).json(); } catch {} }
const ws = new WebSocket(list.find((t) => t.type === "page").webSocketDebuggerUrl);
await new Promise((r) => (ws.onopen = r));
let id = 0; const pend = new Map();
ws.onmessage = (e) => { const m = JSON.parse(e.data); if (m.id && pend.has(m.id)) { pend.get(m.id)(m); pend.delete(m.id); } };
const cdp = (method, params = {}) => new Promise((r) => { const i = ++id; pend.set(i, r); ws.send(JSON.stringify({ id: i, method, params })); });
const ev = async (expr) => (await cdp("Runtime.evaluate", { expression: expr, returnByValue: true, awaitPromise: true })).result?.result?.value;
await cdp("Performance.enable");
await cdp("Emulation.setDeviceMetricsOverride", { width: 412, height: 915, deviceScaleFactor: 2.6, mobile: true });
await cdp("Emulation.setTouchEmulationEnabled", { enabled: true, maxTouchPoints: 5 });
await cdp("Page.addScriptToEvaluateOnNewDocument", { source: `try { localStorage.setItem("wado.client", "Perf-key"); } catch (_) {}
  window.__emits = {}; const iv = setInterval(() => { if (window.dioxus && window.dioxus.send && !window.dioxus.__wrapped) { const o = window.dioxus.send.bind(window.dioxus);
  window.dioxus.send = (m) => { window.__emits[m.type] = (window.__emits[m.type] || 0) + 1; return o(m); }; window.dioxus.__wrapped = true; } }, 50);` });
await cdp("Page.navigate", { url: `http://127.0.0.1:${HTTP}/?relay=http://127.0.0.1:${PORT}&id=${RID}` });
const until = async (expr, ms = 20000) => { const t = Date.now(); while (Date.now() - t < ms) { if (await ev(expr)) return true; await sleep(250); } return false; };
await until(`!!window.__wado && window.__wado.relayUp`);
await ev(`[...document.querySelectorAll("button.go")].find(b => b.textContent.includes("Start"))?.click(); true`);
const up = await until(`!!document.getElementById("wado-bar")`);
console.log("session up:", up, await ev(`document.querySelector(".landingstatus")?.textContent + " | " + [...document.querySelectorAll("button.go")].map(b => b.textContent).join(",")`));
console.log("streaming:", await until(`window.__wado.sessionOn && (document.querySelector("video")?.videoWidth || 0) > 0`, 30000));
await sleep(3000);
const metrics = async () => Object.fromEntries((await cdp("Performance.getMetrics")).result.metrics.map((m) => [m.name, m.value]));
async function phase(name) {
  await ev(`window.__emits = {}; true`);
  const a = await metrics(); await sleep(SECS * 1000); const b = await metrics();
  const d = (k) => b[k] - a[k];
  const emits = await ev(`window.__emits`);
  const v = await ev(`(() => { const v = document.querySelector("video"); const q = v && v.getVideoPlaybackQuality ? v.getVideoPlaybackQuality() : {}; return { on: window.__wado.sessionOn, w: v && v.videoWidth, frames: q.totalVideoFrames, dropped: q.droppedVideoFrames, paused: v && v.paused }; })()`);
  console.log(`\n${name} (${SECS}s) video=${JSON.stringify(v)}`);
  console.log(`  main thread busy ${(100 * d("TaskDuration") / SECS).toFixed(1)}%  script ${(100 * d("ScriptDuration") / SECS).toFixed(1)}%  style ${(d("RecalcStyleCount") / SECS).toFixed(1)}/s (${(1000 * d("RecalcStyleDuration") / SECS).toFixed(1)} ms/s)  layout ${(d("LayoutCount") / SECS).toFixed(1)}/s (${(1000 * d("LayoutDuration") / SECS).toFixed(1)} ms/s)  nodes ${b.Nodes}  heap ${(b.JSHeapUsedSize / 1e6).toFixed(1)} MB`);
  console.log("  bridge emits/s: " + Object.entries(emits || {}).sort((x, y) => y[1] - x[1]).map(([k, v]) => `${k}=${(v / SECS).toFixed(1)}`).join(" "));
}
try {
  await phase("streaming, dock idle");
  await ev(`document.querySelector('[aria-label="Control centre"]').click(); true`);
  await sleep(800);
  await phase("streaming, control centre open");
} finally { for (const p of procs) try { p.kill(); } catch {} process.exit(0); }
