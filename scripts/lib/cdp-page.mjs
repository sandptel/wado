// A headless Chrome page over CDP, serving a built client with `python3 -m http.server`, for the
// checks that need a real browser (scripts/gamepad-check.mjs). It kills only what it started.
//
//   const P = await openPage({ pub, http, cdp })
//   await P.size(w, h, touch) · P.load(query) · P.js(expr) · P.until(expr, ms) · P.shot(file)
//   P.fx(...events) · P.settle() · P.tap(x, y) · P.drag(points) · P.close()

import { spawn } from "node:child_process";
import { mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

export async function openPage({ pub, http = 8793, cdp = 9393 }) {
  const T = mkdtempSync(join(tmpdir(), "wado-cdp-"));
  const procs = [];
  const start = (bin, args) => { const p = spawn(bin, args, { stdio: "ignore" }); procs.push(p); return p; };
  const close = () => { for (const p of procs) try { p.kill("SIGTERM"); } catch {} };
  for (const sig of ["SIGTERM", "SIGINT"]) process.on(sig, () => { close(); process.exit(1); });
  start("python3", ["-m", "http.server", String(http), "--bind", "127.0.0.1", "-d", pub]);
  start("google-chrome-stable", ["--headless=new", "--remote-debugging-port=" + cdp, "--user-data-dir=" + join(T, "chrome"),
    "--no-first-run", "--hide-scrollbars", "about:blank"]);
  let list;
  for (let i = 0; i < 60 && !list; i++) { await sleep(200); try { list = await (await fetch(`http://127.0.0.1:${cdp}/json/list`)).json(); } catch {} }
  const ws = new WebSocket(list.find((t) => t.type === "page").webSocketDebuggerUrl);
  await new Promise((r) => (ws.onopen = r));
  let id = 0;
  const pend = new Map();
  const logs = [];
  ws.onmessage = (e) => {
    const m = JSON.parse(e.data);
    if (m.id && pend.has(m.id)) { pend.get(m.id)(m); pend.delete(m.id); }
    if (m.method === "Runtime.exceptionThrown") logs.push(m.params.exceptionDetails.exception?.description || m.params.exceptionDetails.text);
  };
  const send = (method, params = {}) => new Promise((r) => { const i = ++id; pend.set(i, r); ws.send(JSON.stringify({ id: i, method, params })); });
  const js = async (expr) => {
    const r = await send("Runtime.evaluate", { expression: expr, returnByValue: true, awaitPromise: true });
    if (r.result?.exceptionDetails) throw new Error(r.result.exceptionDetails.exception?.description || JSON.stringify(r.result.exceptionDetails).slice(0, 300));
    return r.result?.result?.value;
  };
  await send("Runtime.enable");
  await send("Page.enable");
  const base = `http://127.0.0.1:${http}/`;
  const P = {
    logs, send, js, sleep,
    async size(w, h, touch = true) {
      await send("Emulation.setDeviceMetricsOverride", { width: w, height: h, deviceScaleFactor: 2, mobile: touch });
      await send("Emulation.setTouchEmulationEnabled", { enabled: touch, maxTouchPoints: touch ? 5 : 0 });
    },
    async load(query) { await send("Page.navigate", { url: base + query }); await P.until("!!window.__wado && !!window.__wado.fixture", 15000); },
    async until(expr, ms = 8000) { const t = Date.now(); while (Date.now() - t < ms) { if (await js(expr).catch(() => false)) return true; await sleep(100); } return false; },
    fx: (...events) => js(`(() => { const W = window.__wado; ${events.map((e) => `W.fixture(${JSON.stringify(e)});`).join("")} return true; })()`),
    async settle(ms = 80) { await sleep(ms); await js(`document.getAnimations().forEach((a) => { try { a.finish(); } catch (_) {} }); true`); await sleep(60); },
    async shot(file) { const r = await send("Page.captureScreenshot", { format: "png" }); writeFileSync(file, Buffer.from(r.result.data, "base64")); },
    // A mouse pointer stands in for a finger: the pad listens for pointer events of any type.
    async drag(points) {
      const [x0, y0] = points[0];
      await send("Input.dispatchMouseEvent", { type: "mousePressed", x: x0, y: y0, button: "left", clickCount: 1 });
      for (const [x, y] of points.slice(1)) { await send("Input.dispatchMouseEvent", { type: "mouseMoved", x, y, button: "left", buttons: 1 }); await sleep(16); }
      const [x1, y1] = points[points.length - 1];
      await send("Input.dispatchMouseEvent", { type: "mouseReleased", x: x1, y: y1, button: "left", clickCount: 1 });
    },
    tap: (x, y) => P.drag([[x, y]]),
    close,
  };
  return P;
}
