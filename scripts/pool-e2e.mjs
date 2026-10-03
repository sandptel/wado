// Two real browsers, a phone and a computer, against a pool of two daemons:
//
//   reload      a reload lands on the home page (no session view), with the running session
//               shown as a tile — not thrown back into the session
//   join        Join session takes it back
//   parallel    the computer, joining while the phone is on, gets the *other* daemon and can
//               start a session of its own (a dropped phone's seat must not fill the pool)
//   tiles       each device's home page lists both sessions, with shape and apps
//   watchdog    a crashed interface reloads itself straight back into the session
//   pairing     a device nobody trusts, opening the QR's link (`wado qr`), is let straight in
//
//   dx build -p wado-client --platform web && cargo build --release -p wado -p wado-relay
//   node scripts/pool-e2e.mjs        (needs a GPU and Chrome; run with the sandbox off)
import { spawn, execFileSync } from "node:child_process";
import { mkdtempSync, mkdirSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const ROOT = new URL("..", import.meta.url).pathname;
const PUB = ROOT + "target/dx/wado-client/debug/web/public";
const PORT = 4996, RID = "999000666", HTTP = 8766;
const T = mkdtempSync(join(tmpdir(), "wado-pool-e2e-"));
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const procs = [];
let failures = 0;
const check = (name, ok, detail = "") => {
  if (!ok) failures++;
  console.log(`${ok ? "  ok  " : "FAIL  "}${name}${ok || !detail ? "" : "\n        " + detail}`);
};
const start = (cmd, args, env = {}) => {
  const p = spawn(cmd, args, { env: { ...process.env, ...env }, stdio: process.env.E2E_LOG && cmd.endsWith("/wado") ? ["ignore", "inherit", "inherit"] : "ignore" });
  procs.push(p);
  return p;
};
mkdirSync(join(T, "cfg/wado"), { recursive: true });
writeFileSync(join(T, "cfg/wado/trusted_clients"), "Phone-key\tPhone\nLaptop-key\tLaptop\n");
start(ROOT + "target/release/wado-relay", ["--bind", "127.0.0.1:" + PORT]);
await sleep(400);
for (const n of ["e2e-pool-a", "e2e-pool-b"]) {
  start(ROOT + "target/release/wado", [], {
    WADO_RELAY_URL: "ws://127.0.0.1:" + PORT, WADO_REMOTE_ID: RID, WADO_INSTANCE: n,
    XDG_CONFIG_HOME: join(T, "cfg"), WADO_UDP_SLICE: n.endsWith("a") ? "4" : "5",
  });
  await sleep(300);
}
start("python3", ["-m", "http.server", String(HTTP), "--bind", "127.0.0.1", "-d", PUB]);

class Browser {
  constructor(name, port, key, mobile, extra = "") { Object.assign(this, { name, port, key, mobile, extra, id: 0, pend: new Map(), errors: [] }); }
  async open() {
    start("google-chrome-stable", ["--headless=new", "--remote-debugging-port=" + this.port,
      "--user-data-dir=" + join(T, "chrome-" + this.name), "--use-fake-ui-for-media-stream",
      "--autoplay-policy=no-user-gesture-required", "--no-first-run", "about:blank"]);
    let list;
    for (let i = 0; i < 50 && !list; i++) { await sleep(200); try { list = await (await fetch(`http://127.0.0.1:${this.port}/json/list`)).json(); } catch {} }
    this.ws = new WebSocket(list.find((t) => t.type === "page").webSocketDebuggerUrl);
    await new Promise((r) => (this.ws.onopen = r));
    this.ws.onmessage = (e) => {
      const m = JSON.parse(e.data);
      if (m.id && this.pend.has(m.id)) { this.pend.get(m.id)(m); this.pend.delete(m.id); }
      else if (m.method === "Runtime.consoleAPICalled") (this.logs ||= []).push(m.params.args.map((a) => a.value ?? a.description).join(" ").slice(0, 300));
      else if (m.method === "Runtime.exceptionThrown") this.errors.push(JSON.stringify(m.params.exceptionDetails).slice(0, 600));
    };
    await this.cdp("Runtime.enable");
    await this.cdp("Page.enable");
    if (this.mobile) {
      await this.cdp("Emulation.setDeviceMetricsOverride", { width: 412, height: 915, deviceScaleFactor: 2.6, mobile: true });
      await this.cdp("Emulation.setTouchEmulationEnabled", { enabled: true, maxTouchPoints: 5 });
    } else {
      await this.cdp("Emulation.setDeviceMetricsOverride", { width: 1440, height: 900, deviceScaleFactor: 1, mobile: false });
    }
    await this.cdp("Page.addScriptToEvaluateOnNewDocument", { source: `try { localStorage.setItem("wado.client", ${JSON.stringify(this.key)}); } catch (_) {}` });
    await this.cdp("Page.navigate", { url: `http://127.0.0.1:${HTTP}/?relay=http://127.0.0.1:${PORT}&id=${RID}${this.extra}` });
  }
  cdp(method, params = {}) { return new Promise((r) => { const i = ++this.id; this.pend.set(i, r); this.ws.send(JSON.stringify({ id: i, method, params })); }); }
  async ev(expr) { return (await this.cdp("Runtime.evaluate", { expression: expr, returnByValue: true, awaitPromise: true })).result?.result?.value; }
  async until(expr, ms = 15000) { const t = Date.now(); while (Date.now() - t < ms) { if (await this.ev(expr)) return true; await sleep(250); } return false; }
  // Click like a finger (or a mouse): at the element's centre, so overlays get their say.
  async press(sel, text = "") {
    const p = await this.ev(`(() => { for (const el of document.querySelectorAll(${JSON.stringify(sel)})) {
      if (${JSON.stringify(text)} && !el.textContent.includes(${JSON.stringify(text)})) continue;
      el.scrollIntoView({ block: "center" }); const r = el.getBoundingClientRect(); if (r.width && r.height) return { x: r.x + r.width / 2, y: r.y + r.height / 2 }; } return null; })()`);
    if (!p) return false;
    if (this.mobile) {
      await this.cdp("Input.dispatchTouchEvent", { type: "touchStart", touchPoints: [p] });
      await sleep(50);
      await this.cdp("Input.dispatchTouchEvent", { type: "touchEnd", touchPoints: [] });
    } else {
      for (const type of ["mousePressed", "mouseReleased"]) await this.cdp("Input.dispatchMouseEvent", { type, x: p.x, y: p.y, button: "left", clickCount: 1 });
    }
    return true;
  }
  inSession() { return this.until(`!!document.getElementById("wado-bar") && document.querySelector("#landing.gone") !== null`); }
  onHome() { return this.until(`document.querySelector("#landing:not(.gone)") !== null && !document.getElementById("wado-bar")`); }
  pool() { return this.ev(`(window.__wado.pool || {}).instance || ""`); }
}

const phone = new Browser("phone", 9341, "Phone-key", true);
const laptop = new Browser("laptop", 9342, "Laptop-key", false);
try {
  await phone.open();
  await phone.until(`!!window.__wado && window.__wado.relayUp`);
  check("phone: Start session", (await phone.press("button.go", "Start session")) && (await phone.inSession()));
  await sleep(2500);
  const phoneDaemon = await phone.pool();

  await phone.cdp("Page.reload");
  check("reload lands on the home page, not the session", await phone.onHome());
  check("home page shows the running session as a tile",
    await phone.until(`!!document.querySelector(".sessioncard") && document.querySelector(".sect")?.textContent.includes("ctive session")`));
  check("the start button offers Join session and New",
    await phone.until(`[...document.querySelectorAll("button.go")].some(b => b.textContent.includes("Join session")) && !!document.querySelector("button.gonew")`));
  phone.logs = [];
  await phone.ev(`(() => { const W = window.__wado; window.__sent = []; const o = W.relaySendMsg; W.relaySendMsg = (m) => { window.__sent.push(m.type); return o(m); };
    const a = W.sessionAct; W.sessionAct = (i, k) => { window.__sent.push("act:" + i + ":" + k + " here=" + (W.pool && W.pool.instance) + " up=" + W.relayUp); return a(i, k); }; return true; })()`);
  check("Join session takes it back", (await phone.press("button.go", "Join session")) && (await phone.inSession(20000)),
    await phone.ev(`(document.querySelector(".landingstatus")?.textContent || "") + " | " + JSON.stringify({ sent: window.__sent, wanted: window.__wado._relayWanted, resuming: window.__wado._relayResuming, on: window.__wado.sessionOn })`));

  if (process.env.E2E_LOG) console.log(phone.logs.join("\n"));
  // The phone drops without a goodbye (a closed tab): its seat must not keep the pool full.
  await laptop.open();
  check("computer gets on while the phone streams", await laptop.until(`!!window.__wado && window.__wado.relayUp`, 20000));
  const lapDaemon = await laptop.pool();
  check("computer is handed the other daemon", lapDaemon && lapDaemon !== phoneDaemon, `phone ${phoneDaemon} laptop ${lapDaemon}`);
  await laptop.until(`!!document.querySelector(".sessioncard")`, 8000);
  const startBtn = (await laptop.ev(`!!document.querySelector("button.gonew")`)) ? ["button.gonew", "New"] : ["button.go", "Start session"];
  check("computer starts a parallel session", (await laptop.press(...startBtn)) && (await laptop.inSession(20000)));
  await sleep(4000);
  await laptop.ev(`window.__wado.leaveSession && document.querySelector('[aria-label="Control centre"]') && true`);

  // Leave running on the laptop, then both sessions are on its home page.
  await laptop.ev(`window.__wado.leaveSession(); true`);
  await laptop.ev(`document.querySelectorAll("#landing").length`);
  check("computer's home page lists both sessions with shape and apps",
    await laptop.until(`document.querySelectorAll(".sessioncard").length === 2 && document.querySelectorAll(".sessionshape span").length === 2`, 15000),
    await laptop.ev(`document.querySelector(".sessions")?.innerText`));

  // Watchdog: a dead interface reloads itself back into the session it was showing.
  await phone.ev(`window.__wado.crashed("e2e: simulated crash"); true`);
  await sleep(1500);
  check("after a crash the phone is back in its session", await phone.inSession(25000));
  const qr = execFileSync(ROOT + "target/release/wado", ["qr", "--relay", "http://x", "--id", RID],
    { env: { ...process.env, XDG_CONFIG_HOME: join(T, "cfg") } }).toString();
  const pair = (qr.match(/pair=([A-Za-z0-9]+)/) || [])[1];
  check("wado qr puts a pairing code in the link", !!pair);
  // A free daemon for it: the laptop closes its tab (its session was left running).
  await laptop.cdp("Page.navigate", { url: "about:blank" });
  await sleep(1500);
  const fresh = new Browser("fresh", 9343, "Stranger-key", true, "&pair=" + pair);
  await fresh.open();
  check("an untrusted device with the QR's code gets straight in", await fresh.until(`!!window.__wado && window.__wado.relayUp`, 20000));
  check("no page exceptions", phone.errors.length + laptop.errors.length === 0, [...phone.errors, ...laptop.errors].join("\n        "));
} finally {
  for (const p of procs) try { p.kill(); } catch {}
  console.log(failures ? `\n${failures} failed` : "\nall passed");
  process.exit(failures ? 1 : 0);
}
