// The passkey step-up in a real browser against real binaries: every way in to "Add a passkey",
// the unlock lifetime a device chooses, and the trackpad's refusal cards. Chrome's virtual
// authenticator answers the real WebAuthn prompts; every click is a real mouse click (a
// programmatic `.click()` walks straight through an `inert` sheet, which is how the sheet's dead
// buttons went unnoticed until 2026-10-30).
//
//   card      Computer page → Connection & trust → "Add a passkey": it makes one and unlocks,
//             and the card itself says so
//   lifetime  while unlocked: "Until I lock it" (with its honest hint) → reconnect → still
//             unlocked → daemon restart → still unlocked → Lock now → locked → a change refused
//             while locked → Unlock from the sheet → `wado passkey lock` → locked on reconnect
//   door      no passkey, `?go=shell`: the shell door's sheet adds one and opens the shell
//   pad       the trackpad's refusals, each an actionable card, never a blank pad: /dev/uinput
//             missing (the daemon runs without it), turned off, locked (Unlock right there),
//             no passkey (Add one right there), no `input` grant (the command, with Copy), and a
//             computer wado cannot drive yet
//   pairing   a freshly code-paired device: the sheet's "Add a passkey"
//   silence   a request the computer never answers: busy gives up after 20 s, with a reason on
//             the card and a button that works again
//   stuck     never stuck (2026-10-30), with the virtual authenticator: a prompt that never
//             answers gives up on its own deadline with Try again; Not now aborts it, so the next
//             Unlock is not refused as "already pending"; the person cancelling; the page going to
//             the background mid-prompt; two challenges for one tap; no matching passkey
//   approve   "Approve on the computer instead": `wado passkey approve --no` turns it away, `--yes`
//             unlocks and opens the door that asked; the request is in the TUI's list meanwhile
//   require   "Ask for my passkey on this phone" off from the phone (only while unlocked): open
//             across a reconnect, the pad opens with no passkey; `wado passkey require … on|off`
//             from the computer
//
//   dx build -p wado-client --platform web && cargo build --release -p wado -p wado-relay
//   node scripts/passkey-ui-e2e.mjs [target/release]       (needs Chrome and bwrap; sandbox off)
//   SHOTS=<dir> keeps the screenshots; WADO_PUB=<dir> serves another client build; ONLY=a,b runs
//   just those phases (after device A pairs).
//
// The daemon runs in bubblewrap with a bare /dev, so it never makes a real uinput device on this
// machine — and the uinput refusal is a real one.
import { spawn, execFileSync } from "node:child_process";
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

const ROOT = new URL("..", import.meta.url).pathname;
const BIN = resolve(ROOT, process.argv[2] || "target/release");
const PUB = process.env.WADO_PUB || ROOT + "target/dx/wado-client/debug/web/public";
const PORT = 4997, RID = "999000666", HTTP = 8769;
const T = mkdtempSync(join(tmpdir(), "wado-pk-ui-"));
const HOME = join(T, "home");
const RUN = join(T, "run");
const SHOTS = process.env.SHOTS || T;
const CONF = join(HOME, ".config/wado");
mkdirSync(SHOTS, { recursive: true });
mkdirSync(CONF, { recursive: true });
mkdirSync(RUN, { mode: 0o700 });
writeFileSync(join(CONF, "trusted_clients"), "");
const env = { HOME, XDG_CONFIG_HOME: join(HOME, ".config"), XDG_RUNTIME_DIR: RUN, XDG_STATE_HOME: join(HOME, ".state"), SHELL: "/bin/sh" };
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const procs = [];
let failures = 0;
const truly = (v) => !!v && !(typeof v === "object" && "thrown" in v);
const check = (name, ok, detail = "") => {
  ok = truly(ok);
  if (!ok) failures++;
  console.log(`${ok ? "  ok  " : "FAIL  "}${name}${ok || !detail ? "" : "\n        " + String(detail).replace(/\n/g, "\n        ")}`);
  return ok;
};
let logs = "";
const start = (cmd, args, e = {}) => {
  const p = spawn(cmd, args, { env: { ...process.env, ...env, ...e }, stdio: ["ignore", "pipe", "pipe"] });
  for (const s of [p.stdout, p.stderr]) s.on("data", (d) => { logs += d.toString().replace(/\x1b\[[0-9;]*m/g, ""); if (process.env.E2E_LOG) process.stdout.write(d); });
  procs.push(p);
  return p;
};
for (const sig of ["SIGTERM", "SIGINT", "SIGHUP"]) process.on(sig, () => { for (const p of procs) try { p.kill(); } catch {} process.exit(1); });
const wado = (...args) => execFileSync(join(BIN, "wado"), args, { env: { ...process.env, ...env } }).toString();
const daemonEnv = { WADO_RELAY_URL: "ws://127.0.0.1:" + PORT, WADO_REMOTE_ID: RID, WADO_INSTANCE: "e2e-pkui", WADO_UDP_SLICE: "6", WADO_NO_DESKTOP_NOTIFY: "1" };
const startDaemon = () => start("bwrap", ["--dev-bind", "/", "/", "--dev", "/dev", "--die-with-parent", join(BIN, "wado"), "daemon"], daemonEnv);
const pkLog = () => logs.split("\n").filter((l) => /passkey|pad: |refused/i.test(l)).slice(-10).join("\n");

class Browser {
  constructor(name, port) { Object.assign(this, { name, port, id: 0, pend: new Map(), errors: [] }); }
  async open(url, key) {
    start("google-chrome-stable", ["--headless=new", "--remote-debugging-port=" + this.port, "--user-data-dir=" + join(T, "chrome-" + this.name),
      "--no-first-run", "--hide-scrollbars", "about:blank"]);
    let list;
    for (let i = 0; i < 50 && !list; i++) { await sleep(200); try { list = await (await fetch(`http://127.0.0.1:${this.port}/json/list`)).json(); } catch {} }
    this.ws = new WebSocket(list.find((t) => t.type === "page").webSocketDebuggerUrl);
    await new Promise((r) => (this.ws.onopen = r));
    this.ws.onmessage = (e) => {
      const m = JSON.parse(e.data);
      if (m.id && this.pend.has(m.id)) { this.pend.get(m.id)(m); this.pend.delete(m.id); }
      else if (m.method === "Runtime.exceptionThrown") this.errors.push(JSON.stringify(m.params.exceptionDetails).slice(0, 600));
    };
    await this.cdp("Runtime.enable");
    await this.cdp("Page.enable");
    await this.cdp("Browser.grantPermissions", { origin: `http://localhost:${HTTP}`, permissions: ["clipboardReadWrite", "clipboardSanitizedWrite"] });
    await this.cdp("Emulation.setDeviceMetricsOverride", { width: 412, height: 915, deviceScaleFactor: 2, mobile: true });
    await this.cdp("Emulation.setFocusEmulationEnabled", { enabled: true });
    await this.cdp("Page.addScriptToEvaluateOnNewDocument", { source: `try { localStorage.setItem("wado.client", ${JSON.stringify(key)}); } catch (_) {}` });
    await this.cdp("WebAuthn.enable", {});
    this.auth = (await this.cdp("WebAuthn.addVirtualAuthenticator", { options: { protocol: "ctap2", transport: "internal",
      hasResidentKey: true, hasUserVerification: true, isUserVerified: true, automaticPresenceSimulation: true } })).result.authenticatorId;
    await this.go(url);
  }
  go(url) { return this.cdp("Page.navigate", { url }); }
  cdp(method, params = {}) { return new Promise((r) => { const i = ++this.id; this.pend.set(i, r); this.ws.send(JSON.stringify({ id: i, method, params })); }); }
  async ev(expr) {
    const r = await this.cdp("Runtime.evaluate", { expression: expr, returnByValue: true, awaitPromise: true });
    if (r.result?.exceptionDetails) return { thrown: r.result.exceptionDetails.exception?.description || "threw" };
    return r.result?.result?.value;
  }
  // A thrown expression is a no, never a truthy `{ thrown }`.
  async until(expr, ms = 15000) { const t = Date.now(); while (Date.now() - t < ms) { if (truly(await this.ev(expr))) return true; await sleep(200); } return false; }
  // A real click — mouse events at the element's centre, once it has stopped moving — so the
  // page sees a user gesture and hit-testing (an `inert` sheet, a scrim on top) is real.
  async click(sel) {
    const at = () => this.ev(`(() => { const e = ${sel}; if (!e) return null; const b = e.getBoundingClientRect(); return { x: b.x + b.width / 2, y: b.y + b.height / 2 }; })()`);
    if (!(await this.ev(`(() => { const e = ${sel}; if (!e) return false; const b = e.getBoundingClientRect(); if (b.top < 0 || b.bottom > innerHeight) e.scrollIntoView({ block: "center" }); else e.scrollIntoView({ block: "nearest", inline: "nearest" }); return true; })()`))) { console.log("        (nothing to click: " + sel.slice(0, 90) + ")"); return false; }
    let r = await at();
    for (let i = 0; i < 8; i++) { await sleep(150); const n = await at(); if (n && r && n.x === r.x && n.y === r.y) break; r = n; }
    if (!r) return false;
    for (const type of ["mousePressed", "mouseReleased"]) await this.cdp("Input.dispatchMouseEvent", { type, x: r.x, y: r.y, button: "left", clickCount: 1 });
    return true;
  }
  async shot(file) {
    const r = await this.cdp("Page.captureScreenshot", { format: "png" });
    if (r.result?.data) writeFileSync(join(SHOTS, file), Buffer.from(r.result.data, "base64"));
  }
  pk() { return this.ev(`JSON.stringify({ state: window.__wado.passkey.state, busy: window.__wado.passkey.busy, need: window.__wado.passkey.need, note: window.__wado.passkey.note })`); }
  up() { return this.until(`!!window.__wado && window.__wado.e2eReady && window.__wado.passkey.state.stay !== undefined`, 25000); }
}

const $ = (s) => `document.querySelector(${JSON.stringify(s)})`;
const byText = (sel, text) => `[...document.querySelectorAll(${JSON.stringify(sel)})].find((e) => e.textContent.includes(${JSON.stringify(text)}))`;
const card = $(".pkcard");
const cardText = `(${card} || {}).textContent || ""`;
const sheetUp = `!!document.querySelector(".pksheet.up")`;
const unlocked = `window.__wado.passkey.state.unlocked`;
const pkState = (k) => `window.__wado.passkey.state[${JSON.stringify(k)}]`;
const padErr = $("#wado-trackpad .tperr");
const padCause = `(((${padErr}) && !(${padErr}).hidden && (${padErr}).dataset.cause) || "")`;

async function openTrust(b) {
  for (let i = 0; i < 4 && !(await b.ev(`!!${card}`)); i++) {
    if (!(await b.ev(`!!document.querySelector("#computer .navrow")`))) await b.click($(".ccard .cpick"));
    await b.until(`!!document.querySelector("#computer .navrow")`, 5000);
    await b.click(byText("#computer .navrow", "Connection"));
    await b.until(`!!${card}`, 4000);
  }
  await b.ev(`(${card} || document.body).scrollIntoView({ block: "center" }); true`);
  await sleep(400);
  return b.ev(`!!${card}`);
}
// Close the passkey sheet if it is up (the pairing offer), as a person would: "Later".
async function later(b) {
  if (await b.ev(sheetUp)) {
    await b.click(byText(".pksheet.up .pkacts .btn", "Later") + ` || ${byText(".pksheet.up .pkacts .btn", "Not now")}`);
    await b.until(`!${sheetUp}`, 4000);
  }
}
const cardShot = async (b, file) => {
  await b.ev(`(${card} || document.body).scrollIntoView({ block: "center" }); true`);
  await sleep(600);
  await b.shot(file);
};
// Elements in `scope` a finger cannot reliably hit: under 44×44 px.
const small = (scope) => `[...document.querySelectorAll(${JSON.stringify(scope)})].filter((e) => { const r = e.getBoundingClientRect(); return r.width && (r.width < 44 || r.height < 44); }).map((e) => e.textContent || e.className)`;

start(join(BIN, "wado-relay"), ["--bind", "127.0.0.1:" + PORT]);
await sleep(400);
let daemon = startDaemon();
start("python3", ["-m", "http.server", String(HTTP), "--bind", "127.0.0.1", "-d", PUB]);
await sleep(1200);
const pairUrl = (...grants) => {
  const link = wado("qr", "--relay", "http://x", "--id", RID, ...grants);
  return `&pair=${(link.match(/pair=([A-Za-z0-9]+)/) || [])[1]}&hk=${(link.match(/hk=([A-Za-z0-9_-]+)/) || [])[1]}`;
};
const BASE = `http://localhost:${HTTP}/?relay=http://127.0.0.1:${PORT}&id=${RID}`;
const only = (process.env.ONLY || "").split(",").filter(Boolean);
const want = (s) => !only.length || only.includes(s);

const a = new Browser("a", 9351);
const b2 = new Browser("b", 9352);
try {
  await a.open(BASE + pairUrl("shells", "input"), "PhoneA-key");
  check("device A pairs with a code", await a.until(`!!window.__wado && window.__wado.e2eReady`, 25000));
  await a.until(sheetUp, 5000);
  await later(a);

  // ── card ───────────────────────────────────────────────────────────────────
  if (want("card") || want("lifetime")) {
    console.log("card");
    check("Connection & trust shows the Passkey card", await openTrust(a));
    check("…with \"Add a passkey\"", await a.until(`!!${byText(".pkcard .btn", "Add a passkey")}`, 5000), await a.ev(cardText));
    await cardShot(a, "pk-card-none.png");
    await a.click(byText(".pkcard .btn", "Add a passkey"));
    check("the card's \"Add a passkey\" makes one and unlocks", await a.until(`window.__wado.passkey.state.registered && ${unlocked}`, 15000), (await a.pk()) + "\n" + pkLog());
    check("…and the card says Unlocked, 15 min by default", await a.until(`${cardText}.includes("Unlocked") && ${cardText}.includes("15 min without use, or when you leave the app")`, 4000), await a.ev(cardText));
    check("the daemon logged the challenge and the passkey", /passkey: challenge sent/.test(logs) && /passkey: added/.test(logs), pkLog());
    await a.until(`!${sheetUp}`, 4000);
    await cardShot(a, "pk-card-unlocked.png");
  }

  // ── lifetime ───────────────────────────────────────────────────────────────
  if (want("lifetime")) {
    console.log("lifetime");
    check("unlocked: the card offers Stay unlocked, four choices and the switch",
      await a.until(`document.querySelectorAll(".pkstay .seg button").length === 4 && !!document.querySelector(".pkstay .sw")`, 4000), await a.ev(cardText));
    check("…the default chosen, no warning", await a.ev(`document.querySelector(".pkstay .seg button.on").textContent === "15 min" && !document.querySelector(".pkhint")`));
    check("…every target 44 px or more", (await a.ev(`${small(".pkstay .seg button, .pkcard .pkacts .btn")}.length`)) === 0, JSON.stringify(await a.ev(small(".pkstay .seg button, .pkcard .pkacts .btn"))));
    await a.click(byText(".pkstay .seg button", "Until I lock"));
    check("\"Until I lock it\": chosen, and leaving the app no longer locks it",
      await a.until(`${pkState("stay")} === "lock" && ${pkState("lock_on_leave")} === false && ${pkState("idle_s")} === 0`, 6000), await a.pk());
    check("…the card says so, with the honest hint", await a.until(`${cardText}.includes("stays unlocked until you lock it") && !!document.querySelector(".pkhint") && /Anyone holding this phone/.test(document.querySelector(".pkhint").textContent)`, 4000), await a.ev(cardText));
    check("…and the computer keeps it", /passkey: unlock lifetime set.*stay="?lock/.test(logs) || /unlock lifetime set/.test(logs), pkLog());
    await cardShot(a, "pk-card-until.png");
    // The page going to the background: with the switch off, it stays unlocked.
    await a.ev(`(() => { Object.defineProperty(document, "visibilityState", { configurable: true, get: () => "hidden" }); document.dispatchEvent(new Event("visibilitychange")); delete document.visibilityState; return true; })()`);
    await sleep(600);
    check("leaving the app with the switch off: still unlocked", await a.ev(unlocked));
    // Reconnect: a new link for the same device.
    await a.go(BASE);
    check("reconnects", await a.up());
    check("a reconnect of the same device: still unlocked, no passkey asked", await a.until(unlocked, 8000) && !(await a.ev(sheetUp)), await a.pk());
    // A daemon restart: "until I lock it" on a code-pinned line comes back from its file.
    daemon.kill("SIGTERM");
    await sleep(1500);
    daemon = startDaemon();
    await sleep(1500);
    await a.go(BASE);
    check("reconnects after a daemon restart", await a.up());
    check("a daemon restart: still unlocked", await a.until(unlocked, 8000), await a.pk());
    check("the computer lists the choice", /until it locks itself/.test(wado("passkey")), wado("passkey"));
    // Lock now.
    await openTrust(a);
    await a.click(byText(".pkcard .btn", "Lock now"));
    check("Lock now: locked", await a.until(`!${unlocked} && ${cardText}.includes("Locked")`, 6000), await a.pk());
    check("…and the choice is hidden while locked", await a.ev(`!document.querySelector(".pkstay")`));
    await cardShot(a, "pk-card-locked.png");
    await a.go(BASE);
    await a.up();
    check("…and stays locked across a reconnect", !(await a.until(unlocked, 3000)), await a.pk());
    // The daemon, not the card, refuses a change while locked.
    await a.ev(`window.__wado.relaySendMsg({ type: "passkey_stay", stay: "8h", lock_on_leave: true })`);
    check("a change sent while locked is refused by the computer", await a.until(`/Unlock with your passkey first/.test(window.__wado.passkey.note)`, 6000), await a.pk());
    check("…and nothing changed", /lifetime not changed/.test(logs) && (await a.ev(`${pkState("stay")} === "lock"`)), pkLog());
    // Unlock from the card: through the sheet, with a real tap on the sheet's button.
    await openTrust(a);
    await a.click(byText(".pkcard .btn", "Unlock"));
    await a.until(sheetUp, 5000);
    await a.click($(".pksheet.up .btn.primary"));
    check("Unlock through the sheet", await a.until(unlocked, 15000), (await a.pk()) + "\n" + pkLog());
    // The computer's own off-ramp.
    const out = wado("passkey", "lock", "PhoneA-key");
    check("`wado passkey lock` says what it did", /locked/.test(out), out);
    await a.go(BASE);
    await a.up();
    check("…and the device is locked on its next connection", !(await a.until(unlocked, 3000)), await a.pk());
    // Back to the default for what follows.
    await openTrust(a);
    await a.click(byText(".pkcard .btn", "Unlock"));
    await a.until(sheetUp, 5000);
    await a.click($(".pksheet.up .btn.primary"));
    await a.until(unlocked, 15000);
    await a.until(`!${sheetUp}`, 4000);
    await a.click(byText(".pkstay .seg button", "15 min"));
    check("back to 15 min: the switch comes back on, the hint goes",
      await a.until(`${pkState("stay")} === "15m" && ${pkState("lock_on_leave")} === true && !document.querySelector(".pkhint")`, 6000), await a.pk());
  }

  // ── door ───────────────────────────────────────────────────────────────────
  if (want("door") || want("pad")) {
    console.log("door");
    wado("passkey", "remove", "PhoneA-key");
    await a.go(BASE + "&go=shell");
    check("reconnects", await a.up());
    check("the shell door asks for a passkey", await a.until(`${sheetUp} && /Add a passkey/.test(document.querySelector(".pksheet.up .btn.primary").textContent)`, 10000),
      (await a.pk()) + "\n" + (await a.ev(`(document.querySelector(".pksheet") || {}).textContent`)));
    await a.click($(".pksheet.up .btn.primary"));
    check("…a real tap adds one, unlocks, and opens the shell", await a.until(`${unlocked} && !!document.querySelector("#console.open")`, 15000), (await a.pk()) + "\n" + pkLog());
  }

  // ── pad ────────────────────────────────────────────────────────────────────
  if (want("pad")) {
    console.log("pad");
    await a.go(BASE + "&go=trackpad");
    await a.up();
    await a.until(unlocked, 6000);
    if (!(await a.ev(`!!document.querySelector("#wado-trackpad:not([hidden])")`))) {
      await a.until(sheetUp, 4000);
      if (await a.ev(sheetUp)) await a.click($(".pksheet.up .btn.primary"));
    }
    check("the trackpad opens", await a.until(`!!document.querySelector("#wado-trackpad:not([hidden])")`, 15000), await a.pk());
    // What crossed the link, for a failure's detail.
    await a.ev(`(() => { const W = window.__wado; const log = (W.__trace = []); const t = () => Date.now() % 100000;
      const s = W.relaySendMsg; W.relaySendMsg = (o) => { if (/^(passkey|pad)_/.test(o.type)) log.push([t(), ">", o.type, o.msg && o.msg.k]); return s(o); };
      for (const k of ["passkey_state", "passkey_challenge", "pad_out", "pad_answer"]) { const h = W._relayHandlers[k]; W._relayHandlers[k] = (m) => { log.push([t(), "<", k, m.msg ? m.msg.k + " " + (m.msg.message || "").slice(0, 24) : m.err ? "err " + m.err.slice(0, 24) : m.state ? "unlocked=" + m.state.unlocked : ""]); return h(m); }; }
      const err = document.querySelector("#wado-trackpad .tperr");
      new MutationObserver(() => log.push([t(), "card", err.hidden ? "hidden" : err.dataset.cause])).observe(err, { attributes: true, attributeFilter: ["hidden", "data-cause"] });
      return true; })()`);
    const trace = async () => JSON.stringify(await a.ev(`window.__wado.__trace.slice(-24)`));
    check("no /dev/uinput: a card with what is wrong and the commands", await a.until(`${padCause} === "uinput"`, 10000), await a.ev(`(${padErr} || {}).textContent`) + "\n" + pkLog());
    check("…each command with its own Copy button", await a.ev(`document.querySelectorAll("#wado-trackpad .tperr .tpcmd code").length >= 3 && document.querySelectorAll("#wado-trackpad .tperr .tpcopy").length === document.querySelectorAll("#wado-trackpad .tperr .tpcmd").length`));
    await sleep(800); // the relayed refusal and the direct offer's both land; the card settles
    await a.click(`document.querySelector("#wado-trackpad .tperr .tpcopy")`);
    check("…and Copy puts it on the phone's clipboard", await a.until(`navigator.clipboard.readText().then((t) => t === "sudo modprobe uinput")`, 4000), JSON.stringify(await a.ev(`navigator.clipboard.readText()`)) + " " + (await trace()));
    check("…its buttons are 44 px or more", (await a.ev(`${small("#wado-trackpad .tperr button")}.length`)) === 0, JSON.stringify(await a.ev(small("#wado-trackpad .tperr button"))));
    await sleep(400);
    await a.shot("pk-pad-uinput.png");

    // Turned off on the computer.
    writeFileSync(join(CONF, "config.kdl"), "trackpad {\n  enabled #false\n}\n");
    wado("msg", "--instance", "e2e-pkui", "reload");
    await a.click(byText("#wado-trackpad .tperr .tpact", "Try again"));
    check("turned off: a card with how to turn it on", await a.until(`${padCause} === "off" && (${padErr}).textContent.includes("wado msg set trackpad.enabled true")`, 8000), await a.ev(`(${padErr} || {}).textContent`));
    await a.shot("pk-pad-off.png");
    rmSync(join(CONF, "config.kdl"));
    wado("msg", "--instance", "e2e-pkui", "reload");

    // Locked from the computer: Unlock on the card itself.
    wado("passkey", "lock", "PhoneA-key");
    await a.click(byText("#wado-trackpad .tperr .tpact", "Try again"));
    check("locked: a card with Unlock", await a.until(`${padCause} === "locked" && !!${byText("#wado-trackpad .tperr .tpact", "Unlock")}`, 8000), await a.ev(`(${padErr} || {}).textContent`) + "\n" + pkLog());
    await a.shot("pk-pad-locked.png");
    await a.click(byText("#wado-trackpad .tperr .tpact", "Unlock"));
    check("…Unlock there unlocks and the pad tries again", await a.until(`${unlocked} && ${padCause} === "uinput"`, 15000), (await a.pk()) + "\n" + (await a.ev(padCause)) + "\n" + (await trace()));

    // No passkey any more: Add one on the card itself.
    // The sheet shows the shackle opening for a moment: let it go before the next tap.
    await a.until(`!${sheetUp}`, 4000);
    wado("passkey", "remove", "PhoneA-key");
    await a.click(byText("#wado-trackpad .tperr .tpact", "Try again"));
    check("no passkey: a card with Add a passkey", await a.until(`${padCause} === "no-passkey" && !!${byText("#wado-trackpad .tperr .tpact", "Add a passkey")}`, 8000), await a.ev(`(${padErr} || {}).textContent`));
    await a.shot("pk-pad-no-passkey.png");
    await a.click(byText("#wado-trackpad .tperr .tpact", "Add a passkey"));
    check("…Add there makes one, and the pad tries again", await a.until(`window.__wado.passkey.state.registered && ${unlocked} && ${padCause} === "uinput"`, 15000), (await a.pk()) + "\n" + (await a.ev(padCause)) + "\n" + (await trace()));

    // A computer wado cannot drive input on (macOS, Windows): its daemon's words.
    await a.ev(`window.__wado._relayHandlers.pad_out({ msg: { k: "err", fatal: true, message: "The trackpad isn't supported on this computer's system yet — wado drives the pointer and keyboard on Linux only, for now." } })`);
    check("another OS: said honestly, nothing to retry", await a.until(`${padCause} === "unsupported" && /Linux for now/.test((${padErr}).textContent) && !(${padErr}).querySelector(".tpact")`, 4000), await a.ev(`(${padErr} || {}).textContent`));
    await a.shot("pk-pad-unsupported.png");
    check("a refusal card's tap does not dismiss it", await (async () => { await a.click(`document.querySelector("#wado-trackpad .tperr .tpwhyt")`); await sleep(300); return a.ev(`${padCause} === "unsupported"`); })());
    wado("allow", "PhoneA-key", "input");
    await a.ev(`window.__wado.padClose(); true`);
  }

  // ── stuck ──────────────────────────────────────────────────────────────────
  // Every way the passkey step can hang or fail, each with a way out (2026-10-30).
  const presence = (on) => a.cdp("WebAuthn.setAutomaticPresenceSimulation", { authenticatorId: a.auth, enabled: on });
  const verified = (on) => a.cdp("WebAuthn.setUserVerified", { authenticatorId: a.auth, isUserVerified: on });
  // What the page asks of WebAuthn, and how each ended.
  const traceWa = () => a.ev(`(() => { const W = window.__wado; if (W.__wa) return true; W.__wa = []; const c = navigator.credentials;
    for (const op of ["get", "create"]) { const f = c[op].bind(c); c[op] = (o) => { const n = W.__wa.length; W.__wa.push({ op, st: "pending" }); const p = f(o);
      p.then(() => (W.__wa[n].st = "ok"), (e) => (W.__wa[n].st = e.name)); return p; }; }
    return true; })()`);
  const wa = () => a.ev(`JSON.stringify(window.__wado.__wa)`);
  const primary = $(".pksheet.up .btn.primary");
  const primaryText = `((${primary}) || {}).textContent || ""`;
  const sheetNote = `((document.querySelector(".pksheet.up .pknote")) || {}).textContent || ""`;
  const lockA = async () => {
    await a.until(`!${sheetUp}`, 4000);
    if (await a.ev(unlocked)) await a.ev(`window.__wado.passkeyLockNow(); true`);
    return a.until(`!${unlocked}`, 5000);
  };
  const askPad = async () => {
    await a.ev(`window.__wado.__padRan = 0; window.__wado.passkeyThen("pad", () => { window.__wado.__padRan++; }); true`);
    return a.until(sheetUp, 5000);
  };
  const ensurePasskey = async () => {
    if (await a.ev(`window.__wado.passkey.state.registered`)) return true;
    await openTrust(a);
    await a.click(byText(".pkcard .btn", "Add a passkey"));
    return a.until(`window.__wado.passkey.state.registered && ${unlocked}`, 15000);
  };

  if (want("stuck") || want("approve") || want("require")) {
    await a.go(BASE);
    await a.up();
    await later(a);
    await presence(true);
    await verified(true);
    check("A has a passkey to work with", await ensurePasskey(), (await a.pk()) + "\n" + pkLog());
    await traceWa();
    await a.ev(`window.__wado.passkeyAuthWaitMs = 6000; true`);
  }

  if (want("stuck")) {
    console.log("stuck");
    // 1. The prompt never answers: its own deadline ends it, with a reason and Try again.
    await lockA();
    await presence(false);
    await askPad();
    await a.click(primary);
    check("never answers: the sheet waits on the passkey", await a.until(`window.__wado.passkey.busy && /Waiting for your passkey/.test(${primaryText})`, 5000), (await a.pk()) + " " + (await a.ev(primaryText)));
    await a.shot("pk-stuck-waiting.png");
    const t0 = Date.now();
    check("…and gives up on its own deadline", await a.until(`!window.__wado.passkey.busy`, 12000), await a.pk());
    check("…in about the deadline (6 s here, 60 s for real)", Date.now() - t0 < 9000, Date.now() - t0 + " ms");
    check("…the prompt itself was aborted, not left pending", await a.until(`!window.__wado.__wa.some((w) => w.st === "pending")`, 3000), await wa());
    check("…with a reason, Try again, and the way round it", await a.until(`/didn't answer in time/.test(${sheetNote}) && /Try again/.test(${primaryText}) && !!${byText(".pksheet.up .btn", "Approve on")}`, 3000), (await a.ev(sheetNote)) + " | " + (await a.ev(primaryText)));
    check("…and the owner's switch, named with the command", await a.ev(`/wado passkey require .* off/.test(document.querySelector(".pksheet.up .pkoff").textContent)`), await a.ev(`(document.querySelector(".pksheet.up .pkoff") || {}).textContent`));
    await sleep(400);
    await a.shot("pk-stuck-timeout.png");

    // 2. Not now while a prompt is up aborts it: the next Unlock is not "already pending".
    await a.click(primary);
    await a.until(`window.__wado.passkey.busy && window.__wado.passkey.stage === "prompt"`, 5000);
    await a.click(byText(".pksheet.up .pkacts .btn", "Not now"));
    check("Not now closes the sheet at once", await a.until(`!${sheetUp} && !window.__wado.passkey.busy`, 3000), await a.pk());
    check("…and takes the prompt down with it", await a.until(`!window.__wado.__wa.some((w) => w.st === "pending")`, 3000), await wa());
    await presence(true);
    await askPad();
    await a.click(primary);
    check("…so the next Unlock works", await a.until(`${unlocked} && window.__wado.__padRan === 1`, 15000), (await a.pk()) + "\n" + (await wa()) + "\n" + pkLog());
    check("…and nothing was refused as already pending", !/already pending/.test(logs), pkLog());

    // 3. The person cancels (here: the screen lock is not confirmed).
    await lockA();
    await verified(false);
    await askPad();
    await a.click(primary);
    check("cancelled: said plainly, Try again", await a.until(`!window.__wado.passkey.busy && /Cancelled/.test(${sheetNote}) && /Try again/.test(${primaryText})`, 10000), (await a.pk()) + " " + (await a.ev(sheetNote)));
    await sleep(400);
    await a.shot("pk-stuck-cancelled.png");
    await verified(true);
    await a.click(primary);
    check("…and Try again unlocks", await a.until(unlocked, 15000), await a.pk());

    // 4. The page goes to the background mid-prompt (the prompt itself hides it on a phone):
    // nothing locks or cancels underneath it.
    await lockA();
    await presence(false);
    await askPad();
    await a.click(primary);
    await a.until(`window.__wado.passkey.stage === "prompt"`, 5000);
    await a.ev(`(() => { Object.defineProperty(document, "visibilityState", { configurable: true, get: () => "hidden" }); document.dispatchEvent(new Event("visibilitychange")); delete document.visibilityState; return true; })()`);
    await sleep(800);
    check("background mid-prompt: still waiting on the passkey, not cancelled", await a.ev(`window.__wado.passkey.busy && window.__wado.passkey.stage === "prompt" && window.__wado.__wa.at(-1).st === "pending"`), (await a.pk()) + " " + (await wa()));
    await a.click(byText(".pksheet.up .pkacts .btn", "Not now"));
    await presence(true);

    // 5. Two challenges for one tap (a double tap, a reconnect): the older prompt gives way to
    // the newer, whose challenge is the one the computer holds — it unlocks, no refusal.
    await a.until(`!${sheetUp}`, 4000);
    await askPad();
    const before = (logs.match(/passkey: unlock refused/g) || []).length;
    await a.ev(`window.__wado.passkeyGo(); window.__wado.passkey.busy = false; window.__wado.passkeyGo(); true`);
    check("double begin: unlocks", await a.until(`${unlocked} && window.__wado.__padRan === 1`, 15000), (await a.pk()) + "\n" + (await wa()) + "\n" + pkLog());
    check("…with no refusal and no prompt left behind", (logs.match(/passkey: unlock refused/g) || []).length === before && (await a.ev(`!window.__wado.__wa.some((w) => w.st === "pending")`)), pkLog() + "\n" + (await wa()));

    // 6. No passkey here matches (one made in another browser or app): said, with the way round.
    await lockA();
    await a.cdp("WebAuthn.clearCredentials", { authenticatorId: a.auth });
    await askPad();
    await a.click(primary);
    check("no matching passkey: says so and offers approve on the computer", await a.until(`!window.__wado.passkey.busy && /no passkey for/.test(${sheetNote}) && !!${byText(".pksheet.up .btn", "Approve on")}`, 10000), (await a.pk()) + " " + (await a.ev(sheetNote)));
    await sleep(400);
    await a.shot("pk-stuck-no-credential.png");
  }

  // ── approve ────────────────────────────────────────────────────────────────
  if (want("approve") || want("stuck")) {
    console.log("approve");
    await lockA();
    await a.until(`!${sheetUp}`, 4000);
    await askPad();
    await a.click(byText(".pksheet.up .btn", "Approve on"));
    check("approve: the sheet waits for the computer, says what to run there", await a.until(`window.__wado.passkey.approve && window.__wado.passkey.approve.state === "waiting" && /wado passkey approve/.test((document.querySelector(".pksheet.up .pkappwait") || {}).textContent || "")`, 6000), (await a.pk()) + " " + JSON.stringify(await a.ev(`window.__wado.passkey.approve`)));
    const listed = wado("passkey", "approve", "--list");
    check("…the request is listed on the computer", /Linux · Chrome/.test(listed) && /s left/.test(listed), listed);
    check("…and logged", /asked to be approved on the computer/.test(logs), pkLog());
    await sleep(400);
    await a.shot("pk-approve-waiting.png");
    const no = wado("passkey", "approve", "--no");
    check("`wado passkey approve --no` turns it away", /turned away/.test(no), no);
    check("…the phone says so, still locked", await a.until(`window.__wado.passkey.approve && window.__wado.passkey.approve.state === "declined" && !${unlocked} && /Turned down/.test(${sheetUp} ? document.querySelector(".pksheet.up").textContent : "")`, 6000), JSON.stringify(await a.ev(`window.__wado.passkey.approve`)));
    await a.shot("pk-approve-declined.png");
    await a.click(byText(".pksheet.up .btn", "Approve on"));
    await a.until(`window.__wado.passkey.approve && window.__wado.passkey.approve.state === "waiting"`, 6000);
    // Piped, as a person types it: y.
    const yes = execFileSync(join(BIN, "wado"), ["passkey", "approve"], { env: { ...process.env, ...env }, input: "y\n" }).toString();
    check("`wado passkey approve` (y) lets it in", /approved/.test(yes), yes);
    check("…the phone unlocks and the door that asked opens", await a.until(`${unlocked} && window.__wado.__padRan === 1 && !${sheetUp}`, 8000), (await a.pk()) + "\n" + pkLog());
    check("…logged as approved", /approved on the computer — unlocked/.test(logs), pkLog());
    check("…the same lifetime as a passkey's (15 min, idle clock running)", await a.ev(`${pkState("idle_s")} === 900`), await a.pk());
    const none = wado("passkey", "approve");
    check("…and nothing waits any more", /nothing is waiting/.test(none), none);
    // Not now withdraws a waiting request.
    await lockA();
    await askPad();
    await a.click(byText(".pksheet.up .btn", "Approve on"));
    await a.until(`window.__wado.passkey.approve && window.__wado.passkey.approve.state === "waiting"`, 6000);
    await a.click(byText(".pksheet.up .pkacts .btn", "Not now"));
    check("Not now withdraws the request", await (async () => { for (let i = 0; i < 20; i++) { if (/nothing is waiting/.test(wado("passkey", "approve", "--no"))) return true; await sleep(200); } return false; })(), wado("passkey", "approve", "--no"));
  }

  // ── require ────────────────────────────────────────────────────────────────
  if (want("require") || want("stuck")) {
    console.log("require");
    // The switch is offered only while unlocked; turning it off is the phone's own choice.
    await a.until(`!${sheetUp}`, 4000);
    await a.ev(`window.__wado.relaySendMsg({ type: "passkey_require", on: false }); true`);
    check("off is refused while locked", await a.until(`/Unlock first/.test(window.__wado.passkey.note)`, 5000), await a.pk());
    // Unlock — by approval: A's credential was cleared above.
    await askPad();
    await a.click(byText(".pksheet.up .btn", "Approve on"));
    await a.until(`window.__wado.passkey.approve && window.__wado.passkey.approve.state === "waiting"`, 6000);
    wado("passkey", "approve", "--yes");
    await a.until(unlocked, 8000);
    await openTrust(a);
    check("unlocked: the card offers \"Ask for my passkey on this phone\"", await a.until(`!!document.querySelector(".pkreq .sw.on")`, 4000), await a.ev(cardText));
    await a.click($(".pkreq .sw"));
    check("…off: not asked, said honestly on the card", await a.until(`${pkState("required")} === false && ${unlocked} && ${cardText}.includes("Not asked") && /Anyone holding this phone/.test(document.querySelector(".pkreq .pkhint").textContent)`, 6000), (await a.pk()) + "\n" + (await a.ev(cardText)));
    check("…logged as the device's choice", /no longer asked — turned off by the device/.test(logs), pkLog());
    check("…and listed on the computer", /passkey not asked/.test(wado("passkey")), wado("passkey"));
    await cardShot(a, "pk-require-off.png");
    await a.go(BASE + "&go=trackpad");
    await a.up();
    check("a reconnect: still open, no sheet", await a.until(unlocked, 8000) && !(await a.ev(sheetUp)), await a.pk());
    check("…the trackpad opens without a passkey", await a.until(`!!document.querySelector("#wado-trackpad:not([hidden])") && ${padCause} !== "locked" && ${padCause} !== "no-passkey"`, 10000), (await a.pk()) + " " + (await a.ev(padCause)));
    await a.ev(`window.__wado.padClose && window.__wado.padClose(); true`);
    // Back on, from the computer: asked again at once.
    const on = wado("passkey", "require", "PhoneA-key", "on");
    check("`wado passkey require … on` says what it did", /asked for its passkey again/.test(on), on);
    await a.ev(`window.__wado.relaySendMsg({ type: "passkey_get" }); true`);
    check("…and the phone is asked again", await a.until(`${pkState("required")} === true && !${unlocked}`, 6000), await a.pk());
    // Off from the computer, while the phone is locked: the owner's way out.
    const off = wado("passkey", "require", "PhoneA-key", "off");
    check("`wado passkey require … off` warns", /no longer asked/.test(off) && /Anyone holding it/.test(off), off);
    await a.ev(`window.__wado.relaySendMsg({ type: "passkey_get" }); true`);
    check("…the locked phone is open", await a.until(`${pkState("required")} === false && ${unlocked}`, 6000), await a.pk());
    wado("passkey", "require", "PhoneA-key", "on");
    // A real passkey again for the phases after this one.
    wado("passkey", "remove", "PhoneA-key");
    await a.ev(`window.__wado.relaySendMsg({ type: "passkey_get" }); true`);
    await a.until(`!window.__wado.passkey.state.registered`, 5000);
    await ensurePasskey();
    await a.ev(`window.__wado.passkeyAuthWaitMs = 0; true`);
  }

  // ── pairing ────────────────────────────────────────────────────────────────
  if (want("pairing") || want("silence")) {
    console.log("pairing");
    // One seat on this daemon: A lets go of it first.
    await a.go("about:blank");
    await sleep(2500);
    await b2.open(BASE + pairUrl("shells"), "PhoneB-key");
    check("device B pairs with a code", await b2.until(`!!window.__wado && window.__wado.e2eReady`, 25000));
    check("…and ends on the passkey sheet", await b2.until(`${sheetUp} && /One last step/.test(document.querySelector("#pktitle").textContent)`, 10000));
    await sleep(500);
    await b2.shot("pk-pairing-sheet.png");
    await b2.click($(".pksheet.up .btn.primary"));
    check("the sheet's \"Add a passkey\" makes one and unlocks", await b2.until(`window.__wado.passkey.state.registered && ${unlocked}`, 15000), (await b2.pk()) + "\n" + pkLog());

    // No `input` grant: B was paired with shells only (A is the owner, which has every grant).
    await b2.until(`!${sheetUp}`, 4000);
    await b2.ev(`window.__wado.padOpen(); true`);
    const bErr = $("#wado-trackpad .tperr");
    const bCause = `(((${bErr}) && !(${bErr}).hidden && (${bErr}).dataset.cause) || "")`;
    check("no input grant: the exact command, the device's name quoted", await b2.until(`${bCause} === "no-grant" && (${bErr}).querySelector(".tpcmd code").textContent === 'wado allow "Linux · Chrome" input'`, 10000), await b2.ev(`(${bErr} || {}).textContent`));
    check("…and the other way, wado tui", await b2.ev(`(${bErr}).textContent.includes("wado tui")`));
    await b2.click(`document.querySelector("#wado-trackpad .tperr .tpcopy")`);
    check("…Copy puts the command on the clipboard", await b2.until(`navigator.clipboard.readText().then((t) => t === 'wado allow "Linux · Chrome" input')`, 4000), await b2.ev(`navigator.clipboard.readText()`));
    check("…and says so", await b2.until(`document.querySelector("#wado-trackpad .tperr .tpcopy").textContent === "Copied"`, 2000));
    await sleep(300);
    await b2.shot("pk-pad-no-grant.png");
    await b2.ev(`window.__wado.padClose(); true`);
  }

  // ── silence ────────────────────────────────────────────────────────────────
  if (want("silence")) {
    console.log("silence");
    await b2.until(`!${sheetUp}`, 4000);
    await openTrust(b2);
    if (await b2.ev(unlocked)) await b2.click(byText(".pkcard .btn", "Lock now"));
    await b2.until(`!${unlocked}`, 5000);
    // The computer goes quiet: what is sent arrives nowhere, and nothing comes back.
    await b2.ev(`(() => { const W = window.__wado; W.__real = W.relaySendMsg; W.relaySendMsg = (o) => (/^passkey_/.test(o.type) ? true : W.__real(o));
      W.__state = W._relayHandlers.passkey_state; W._relayHandlers.passkey_state = () => {}; return true; })()`);
    await b2.click(byText(".pkcard .btn", "Unlock"));
    await b2.until(sheetUp, 5000);
    await b2.click($(".pksheet.up .btn.primary"));
    check("waiting: busy, on the sheet and the card", await b2.until(`window.__wado.passkey.busy && /Waiting/.test(${cardText})`, 3000), await b2.pk());
    const t0 = Date.now();
    check("silence: after 20 s busy gives up", await b2.until(`!window.__wado.passkey.busy`, 26000), await b2.pk());
    const took = Date.now() - t0;
    check("…in about 20 s", took > 15000 && took < 25000, took + " ms");
    check("…with the reason on the sheet and on the card", await b2.until(`/didn't answer/.test(${cardText}) && /didn't answer/.test(document.querySelector(".pksheet.up .pknote").textContent)`, 3000), await b2.ev(cardText));
    check("…and buttons that work again", await b2.ev(`!${byText(".pkcard .btn", "Unlock")}.disabled && !document.querySelector(".pksheet.up .btn.primary").disabled`));
    await sleep(400);
    await b2.shot("pk-sheet-silence.png");
    await b2.ev(`(() => { const W = window.__wado; W.relaySendMsg = W.__real; W._relayHandlers.passkey_state = W.__state; return true; })()`);
    await b2.click($(".pksheet.up .btn.primary"));
    check("the computer back: Unlock works", await b2.until(unlocked, 15000), await b2.pk());
  }
  check("no script errors", !a.errors.length && !b2.errors.length, [...a.errors, ...b2.errors].join("\n"));
} finally {
  for (const p of procs) try { p.kill("SIGTERM"); } catch {}
}
if (failures) console.log("\n" + pkLog());
console.log(`\nscreenshots in ${SHOTS}`);
console.log(failures ? `${failures} FAILED` : "all passed");
process.exit(failures ? 1 : 0);
