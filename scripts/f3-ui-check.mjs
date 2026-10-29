// F3 in a real browser: the passkey sheet after pairing (Chrome's virtual authenticator answers
// the real WebAuthn prompt), the doors behind it, Claude's cards with Allow from the page, and the
// Notifications card — with screenshots of each.
//
//   dx build -p wado-client --platform web && cargo build -p wado -p wado-relay
//   node scripts/f3-ui-check.mjs [target/debug]      (needs Chrome; run with the sandbox off)
//
// SHOTS=<dir> keeps the screenshots (default: the run's temp dir).

import { spawn, execFileSync } from "node:child_process";
import { mkdtempSync, mkdirSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const ROOT = new URL("..", import.meta.url).pathname;
const BIN = join(ROOT, process.argv[2] || "target/debug");
const PUB = ROOT + "target/dx/wado-client/debug/web/public";
const PORT = 4993, RID = "999000444", HTTP = 8771, CDP = 9340;
const T = mkdtempSync(join(tmpdir(), "wado-f3-ui-"));
const HOME = join(T, "home");
const RUN = join(T, "run");
const SHOTS = process.env.SHOTS || T;
mkdirSync(SHOTS, { recursive: true });
mkdirSync(join(HOME, ".config/wado"), { recursive: true });
mkdirSync(join(HOME, "code/api"), { recursive: true });
mkdirSync(RUN, { mode: 0o700 });
writeFileSync(join(HOME, ".config/wado/trusted_clients"), "");
const env = { HOME, XDG_CONFIG_HOME: join(HOME, ".config"), XDG_RUNTIME_DIR: RUN, XDG_STATE_HOME: join(HOME, ".state"), SHELL: "/bin/sh" };
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const procs = [];
let failures = 0;
const check = (name, ok, detail = "") => {
  if (!ok) failures++;
  console.log(`${ok ? "  ok  " : "FAIL  "}${name}${ok || !detail ? "" : "\n        " + detail}`);
};
const start = (cmd, args, e = {}) => {
  const p = spawn(cmd, args, { env: { ...process.env, ...env, ...e }, stdio: ["ignore", "pipe", "pipe"] });
  p.log = "";
  for (const s of [p.stdout, p.stderr]) s.on("data", (d) => { p.log += d; if (process.env.E2E_LOG) process.stdout.write(d); });
  procs.push(p);
  return p;
};
for (const sig of ["SIGTERM", "SIGINT"]) process.on(sig, () => { for (const p of procs) try { p.kill(); } catch {} process.exit(1); });
const wado = (...args) => execFileSync(join(BIN, "wado"), args, { env: { ...process.env, ...env } }).toString();
function hook(event) {
  const p = spawn(join(BIN, "wado"), ["claude-event"], { env: { ...process.env, ...env, TMUX: "" }, stdio: ["pipe", "pipe", "pipe"] });
  p.out = "";
  p.stdout.on("data", (d) => (p.out += d));
  p.done = new Promise((r) => p.on("exit", r));
  p.stdin.end(JSON.stringify(event));
  return p;
}

class Browser {
  constructor() { Object.assign(this, { id: 0, pend: new Map(), errors: [] }); }
  async open(url) {
    start("google-chrome-stable", ["--headless=new", "--remote-debugging-port=" + CDP, "--user-data-dir=" + join(T, "chrome"),
      "--no-first-run", "--hide-scrollbars", "about:blank"]);
    let list;
    for (let i = 0; i < 50 && !list; i++) { await sleep(200); try { list = await (await fetch(`http://127.0.0.1:${CDP}/json/list`)).json(); } catch {} }
    this.ws = new WebSocket(list.find((t) => t.type === "page").webSocketDebuggerUrl);
    await new Promise((r) => (this.ws.onopen = r));
    this.ws.onmessage = (e) => {
      const m = JSON.parse(e.data);
      if (m.id && this.pend.has(m.id)) { this.pend.get(m.id)(m); this.pend.delete(m.id); }
      else if (m.method === "Runtime.exceptionThrown") this.errors.push(JSON.stringify(m.params.exceptionDetails).slice(0, 600));
    };
    await this.cdp("Runtime.enable");
    await this.cdp("Page.enable");
    await this.cdp("Emulation.setDeviceMetricsOverride", { width: 412, height: 915, deviceScaleFactor: 2, mobile: true });
    await this.cdp("Emulation.setTouchEmulationEnabled", { enabled: true, maxTouchPoints: 5 });
    await this.cdp("Page.addScriptToEvaluateOnNewDocument", { source: `try { localStorage.setItem("wado.client", "Phone-key"); } catch (_) {}` });
    await this.cdp("WebAuthn.enable", {});
    await this.cdp("WebAuthn.addVirtualAuthenticator", { options: { protocol: "ctap2", transport: "internal",
      hasResidentKey: true, hasUserVerification: true, isUserVerified: true, automaticPresenceSimulation: true } });
    await this.cdp("Page.navigate", { url });
  }
  cdp(method, params = {}) { return new Promise((r) => { const i = ++this.id; this.pend.set(i, r); this.ws.send(JSON.stringify({ id: i, method, params })); }); }
  async ev(expr) {
    const r = await this.cdp("Runtime.evaluate", { expression: expr, returnByValue: true, awaitPromise: true });
    if (r.result?.exceptionDetails) return { thrown: r.result.exceptionDetails.exception?.description || "threw" };
    return r.result?.result?.value;
  }
  async until(expr, ms = 15000) { const t = Date.now(); while (Date.now() - t < ms) { if (await this.ev(expr)) return true; await sleep(200); } return false; }
  async click(sel) { return this.ev(`(() => { const e = document.querySelector(${JSON.stringify(sel)}); if (!e) return false; e.click(); return true; })()`); }
  async shot(file) {
    const r = await this.cdp("Page.captureScreenshot", { format: "png" });
    if (r.result?.data) writeFileSync(join(SHOTS, file), Buffer.from(r.result.data, "base64"));
  }
}

const b = new Browser();
try {
  start(join(BIN, "wado-relay"), ["--bind", "127.0.0.1:" + PORT]);
  await sleep(400);
  const daemon = start(join(BIN, "wado"), ["daemon"], { WADO_RELAY_URL: "ws://127.0.0.1:" + PORT, WADO_REMOTE_ID: RID, WADO_INSTANCE: "e2e-f3ui", WADO_UDP_SLICE: "4" });
  start("python3", ["-m", "http.server", String(HTTP), "--bind", "127.0.0.1", "-d", PUB]);
  await sleep(900);
  const link = wado("qr", "--relay", "http://x", "--id", RID, "shells", "input");
  const pair = (link.match(/pair=([A-Za-z0-9]+)/) || [])[1];
  const hk = (link.match(/hk=([A-Za-z0-9_-]+)/) || [])[1];
  await b.open(`http://localhost:${HTTP}/?relay=http://127.0.0.1:${PORT}&id=${RID}&pair=${pair}&hk=${hk}`);

  console.log("passkey");
  check("pairs with the QR's code", await b.until(`!!window.__wado && window.__wado.e2eReady`, 25000));
  check("the pairing ends on the passkey sheet", await b.until(`!!document.querySelector(".pksheet.up") && /One last step/.test(document.querySelector("#pktitle").textContent)`, 10000));
  await sleep(700);
  await b.shot("01-paired-passkey-sheet.png");
  await b.click(".pksheet.up .btn.primary");
  check("the virtual authenticator makes the passkey; the computer verifies it and unlocks",
    await b.until(`window.__wado.passkey.state.registered && window.__wado.passkey.state.unlocked`, 15000),
    JSON.stringify(await b.ev(`window.__wado.passkey`)));
  await sleep(250);
  await b.shot("02-unlocked.png");
  check("the sheet leaves after the shackle opens", await b.until(`!document.querySelector(".pksheet.up")`, 4000));

  console.log("claude");
  // Claude's activity, through the real hook command.
  await hook({ session_id: "s1", hook_event_name: "UserPromptSubmit", cwd: join(HOME, "code/api"), prompt: "make the flaky upload test deterministic" }).done;
  await hook({ session_id: "s2", hook_event_name: "Stop", cwd: join(HOME, "code/site"), last_assistant_message: "Deployed the preview; all 214 checks pass." }).done;
  const asking = hook({ session_id: "s1", hook_event_name: "PermissionRequest", cwd: join(HOME, "code/api"), tool_name: "Bash", tool_input: { command: "cargo test -p wado --lib files::recv" } });
  // Open the computer's page from its card.
  check("home shows the computer", await b.until(`!!document.querySelector(".ccard .cpick")`, 15000));
  await b.click(".ccard .cpick");
  check("its page shows Claude's cards", await b.until(`document.querySelectorAll(".clcard").length >= 2`, 15000),
    await b.ev(`document.querySelector("#computer") && document.querySelector("#computer").innerText.slice(0, 400)`));
  check("…the one asking with its question", await b.until(`!!document.querySelector(".clcard[data-state=permission] .clask code")`, 8000));
  await b.ev(`document.getElementById("claude").scrollIntoView({ block: "start" }); true`);
  await sleep(700);
  await b.shot("03-claude-cards.png");
  // Allow, as a person would: the phone is unlocked, so it goes straight through.
  await b.click(".clcard[data-state=permission] .clbtns .btn.primary");
  await Promise.race([asking.done, sleep(8000)]);
  check("Allow on the card reaches the waiting hook", /"behavior":"allow"/.test(asking.out), asking.out);
  check("…and the card goes back to working", await b.until(`!document.querySelector(".clcard[data-state=permission]")`, 8000));

  console.log("relock");
  await b.ev(`window.__wado.passkeyLockNow(); true`);
  check("locked", await b.until(`!window.__wado.passkey.state.unlocked`, 4000));
  await b.click("#computer .acts .act:nth-child(4)");
  check("the Shell door asks for the passkey", await b.until(`!!document.querySelector(".pksheet.up") && /Unlock with your passkey/.test(document.querySelector("#pktitle").textContent)`, 6000));
  await sleep(600);
  await b.shot("04-shell-door-locked.png");
  await b.click(".pksheet.up .btn.primary");
  check("unlocking opens the shell it was asked for", await b.until(`window.__wado.passkey.state.unlocked && !!document.querySelector("#console.open")`, 15000));

  console.log("trust page");
  await b.ev(`document.querySelector("#console .iconbtn[aria-label*=Close], #console button[aria-label*=close]")?.click(); true`);
  const opened = await b.ev(`(() => { const r = [...document.querySelectorAll("#computer .navrow")].find((e) => /Connection/.test(e.textContent)); if (!r) return false; r.click(); return true; })()`);
  check("Connection & trust opens", opened && await b.until(`!!document.querySelector(".pkcard") && !!document.querySelector(".pushcard")`, 8000));
  await sleep(700);
  await b.ev(`document.querySelector(".pkcard").scrollIntoView({ block: "center" }); true`);
  await sleep(300);
  await b.shot("05-connection-trust.png");
  check("no script errors", b.errors.length === 0, b.errors.join("\n"));
  if (failures) console.log(daemon.log.split("\n").filter((l) => /passkey|claude|push/i.test(l)).slice(-20).join("\n"));
} finally {
  for (const p of procs) try { p.kill("SIGTERM"); } catch {}
}
console.log(`screenshots: ${SHOTS}`);
console.log(failures ? `\n${failures} failed` : "\nall passed");
process.exit(failures ? 1 : 0);
