// The computer's clipboard, end to end, in two real browsers: the Clipboard surface on the
// Computer page, and sharing both ways by itself (F10).
//
// "The computer's desktop" is a second wado daemon's Wayland socket: its compositor speaks
// data-control, so `wl-copy`, `wl-paste --watch cliphist store` and cliphist work against it
// exactly as on a real desktop — and the person's own clipboard is never touched.
//
//   history     a desktop copy appears, as text, link or picture, said whole to a screen reader
//   switches    off by default for a device trusted before pairing codes, on for a code-paired one
//   auto        a desktop copy lands on the phone by itself; in the background it waits for focus;
//               a refused write becomes a one-tap chip; the phone's copy goes to the computer on
//               focus, once, with no echo either way
//   secrets     `wl-copy --sensitive` never reaches the phone nor its history
//   pin/search  pinned on top; search and kinds narrow the list
//   per entry   Copy to the phone (with a tick), Send back to the computer, Delete (cliphist too)
//   send        "Send this phone's clipboard": text, then a picture
//   a11y        every button named, every target 44 px
//   grant       without `clipboard`: the command that allows it; with it, sharing just works
//
//   dx build -p wado-client --platform web && cargo build --release -p wado -p wado-relay
//   node scripts/clip-e2e.mjs [target/release]      (needs Chrome, wl-clipboard ≥ 2.2, cliphist; sandbox off)
//   SHOTS=<dir> keeps the screenshots; WADO_PUB=<dir> serves another client build.
import { spawn, execFileSync } from "node:child_process";
import { mkdtempSync, mkdirSync, writeFileSync } from "node:fs";
import { deflateSync } from "node:zlib";
import { tmpdir } from "node:os";
import { join } from "node:path";

const ROOT = new URL("..", import.meta.url).pathname;
const BIN = join(ROOT, process.argv[2] || "target/release");
const PUB = process.env.WADO_PUB || ROOT + "target/dx/wado-client/debug/web/public";
const PORT = 4996, RID = "999000555", HTTP = 8768;
const T = mkdtempSync(join(tmpdir(), "wado-clip-e2e-"));
const HOME = join(T, "home");
const SHOTS = process.env.SHOTS || T;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const procs = [];
let failures = 0;
const check = (name, ok, detail = "") => {
  if (!ok) failures++;
  console.log(`${ok ? "  ok  " : "FAIL  "}${name}${ok || !detail ? "" : "\n        " + detail}`);
};
for (const sig of ["SIGTERM", "SIGINT", "SIGHUP"]) process.on(sig, () => { for (const p of procs) try { p.kill(); } catch {} process.exit(1); });

const env = {
  HOME, XDG_CONFIG_HOME: join(HOME, ".config"), XDG_DATA_HOME: join(HOME, ".local/share"),
  XDG_STATE_HOME: join(HOME, ".local/state"), XDG_CACHE_HOME: join(HOME, ".cache"),
};
mkdirSync(join(HOME, ".config/wado"), { recursive: true });
// The phone is already trusted (a line from before keys) and, as the first line, the owner. No
// device gets in unpaired (Decision Log 2026-10-27); the laptop pairs with a QR code below.
writeFileSync(join(HOME, ".config/wado/trusted_clients"), "Phone-key\tPhone\n");

let logs = "";
const start = (cmd, args, e = {}) => {
  const p = spawn(cmd, args, { env: { ...process.env, ...env, ...e }, stdio: ["ignore", "pipe", "pipe"] });
  for (const s of [p.stdout, p.stderr]) s.on("data", (d) => { logs += d.toString().replace(/\x1b\[[0-9;]*m/g, ""); if (process.env.E2E_LOG) process.stdout.write(d); });
  procs.push(p);
  return p;
};

// The stand-in desktop: a direct-mode daemon, for its Wayland socket.
start(join(BIN, "wado"), ["daemon", "127.0.0.1:8772"], { WADO_INSTANCE: "e2e-clip-desk", WAYLAND_DISPLAY: "" });
let desk = "";
for (let i = 0; i < 60 && !desk; i++) { await sleep(200); desk = (logs.match(/Created new socket name=Some\("(wayland-\d+)"\)/) || [])[1] || ""; }
if (!desk) { console.log("FAIL  the stand-in desktop did not come up\n" + logs.slice(-2000)); process.exit(1); }
const D = { ...process.env, ...env, WAYLAND_DISPLAY: desk };
const deskCopy = (data, type) => execFileSync("wl-copy", type ? ["--type", type] : [], { env: D, input: data, stdio: ["pipe", "ignore", "ignore"] });
const deskPaste = (...a) => execFileSync("wl-paste", a, { env: D }).toString();
const history = () => execFileSync("cliphist", ["list"], { env: D }).toString();
start("wl-paste", ["--watch", "cliphist", "store"], { WAYLAND_DISPLAY: desk });

start(join(BIN, "wado-relay"), ["--bind", "127.0.0.1:" + PORT]);
await sleep(400);
// Two daemons: one device each. Staggered: on a fresh config, two at once race to create the
// host key and one dies of the half-written file.
for (const n of [4, 5]) {
  start(join(BIN, "wado"), ["daemon"], {
    WADO_RELAY_URL: "ws://127.0.0.1:" + PORT, WADO_REMOTE_ID: RID, WADO_INSTANCE: "e2e-clip-" + n, WADO_UDP_SLICE: String(n), WAYLAND_DISPLAY: desk,
  });
  await sleep(800);
}
start("python3", ["-m", "http.server", String(HTTP), "--bind", "127.0.0.1", "-d", PUB]);
await sleep(1200);

class Browser {
  constructor(name, port, key, mobile, extra = "") { Object.assign(this, { name, port, key, mobile, extra, id: 0, pend: new Map(), errors: [] }); }
  async open() {
    start("google-chrome-stable", ["--headless=new", "--remote-debugging-port=" + this.port,
      "--user-data-dir=" + join(T, "chrome-" + this.name), "--no-first-run", "about:blank"]);
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
    await this.cdp("Browser.grantPermissions", { origin: `http://127.0.0.1:${HTTP}`, permissions: ["clipboardReadWrite", "clipboardSanitizedWrite"] });
    await this.cdp("Emulation.setFocusEmulationEnabled", { enabled: true });
    await this.cdp("Emulation.setDeviceMetricsOverride", this.mobile
      ? { width: 412, height: 915, deviceScaleFactor: 2, mobile: true } : { width: 1440, height: 900, deviceScaleFactor: 1, mobile: false });
    await this.cdp("Page.addScriptToEvaluateOnNewDocument", { source: `try { localStorage.setItem("wado.client", ${JSON.stringify(this.key)}); } catch (_) {}` });
    await this.cdp("Page.navigate", { url: `http://127.0.0.1:${HTTP}/?relay=http://127.0.0.1:${PORT}&id=${RID}${this.extra}` });
  }
  cdp(method, params = {}) { return new Promise((r) => { const i = ++this.id; this.pend.set(i, r); this.ws.send(JSON.stringify({ id: i, method, params })); }); }
  async ev(expr) {
    const r = await this.cdp("Runtime.evaluate", { expression: expr, returnByValue: true, awaitPromise: true, userGesture: true });
    if (r.result?.exceptionDetails) return { thrown: r.result.exceptionDetails.exception?.description || "threw" };
    return r.result?.result?.value;
  }
  async until(expr, ms = 15000) { const t = Date.now(); while (Date.now() - t < ms) { if (await this.ev(expr)) return true; await sleep(250); } return false; }
  // A real click — mouse events at the element's centre — so the page sees a user gesture.
  async click(sel) {
    const r = await this.ev(`(() => { const e = ${sel}; if (!e) return null; e.scrollIntoView({ block: "center" }); const b = e.getBoundingClientRect(); return { x: b.x + b.width / 2, y: b.y + b.height / 2 }; })()`);
    if (!r) return false;
    await sleep(120);
    // Tiles glide for ~400 ms after the list changes; aim where it settles.
    const at = await this.ev(`(() => { const b = (${sel}).getBoundingClientRect(); return { x: b.x + b.width / 2, y: b.y + b.height / 2 }; })()`);
    if (at && (at.x !== r.x || at.y !== r.y)) { await sleep(500); Object.assign(r, await this.ev(`(() => { const b = (${sel}).getBoundingClientRect(); return { x: b.x + b.width / 2, y: b.y + b.height / 2 }; })()`)); }
    for (const type of ["mousePressed", "mouseReleased"]) await this.cdp("Input.dispatchMouseEvent", { type, x: r.x, y: r.y, button: "left", clickCount: 1 });
    return true;
  }
  async shot(file) {
    const r = await this.cdp("Page.captureScreenshot", { format: "png" });
    if (r.result?.data) writeFileSync(join(SHOTS, file), Buffer.from(r.result.data, "base64"));
  }
}

const tile = (text) => `[...document.querySelectorAll("#clipsec .cliptile")].find((t) => t.textContent.includes(${JSON.stringify(text)}))`;
const visible = () => `[...document.querySelectorAll("#clipsec .cliptile:not(.leaving)")].map((t) => t.className.split(" ")[1])`;
const sw = (n) => `document.querySelectorAll("#clipsec .clipswitches .sw")[${n}]`;
const dock = `(document.querySelector("#clipdock") || {}).textContent || ""`;
const phoneClip = `navigator.clipboard.readText()`;
// A 48×48 PNG, made here.
const png = (() => {
  const crc = (b) => { let c = ~0; for (const x of b) { c ^= x; for (let k = 0; k < 8; k++) c = c & 1 ? (c >>> 1) ^ 0xedb88320 : c >>> 1; } return ~c >>> 0; };
  const chunk = (t, d) => { const l = Buffer.alloc(4); l.writeUInt32BE(d.length); const td = Buffer.concat([Buffer.from(t), d]); const c = Buffer.alloc(4); c.writeUInt32BE(crc(td)); return Buffer.concat([l, td, c]); };
  const ihdr = Buffer.alloc(13); ihdr.writeUInt32BE(48, 0); ihdr.writeUInt32BE(48, 4); ihdr[8] = 8; ihdr[9] = 2;
  const raw = Buffer.alloc(48 * (1 + 48 * 3)); for (let y = 0; y < 48; y++) for (let x = 0; x < 48; x++) raw.set([x * 5, y * 5, 160], y * 145 + 1 + x * 3);
  return Buffer.concat([Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]), chunk("IHDR", ihdr), chunk("IDAT", deflateSync(raw)), chunk("IEND", Buffer.alloc(0))]);
})();
const deskWait = async (want) => {
  let got = "";
  for (let i = 0; i < 40 && got !== want; i++) { await sleep(250); try { got = deskPaste("--no-newline"); } catch {} }
  return got;
};

const phone = new Browser("phone", 9341, "Phone-key", true);
const qr = execFileSync(join(BIN, "wado"), ["qr", "--relay", "http://x", "--id", RID], { env: { ...process.env, ...env } }).toString();
const laptop = new Browser("laptop", 9342, "Laptop-key", false,
  `&pair=${(qr.match(/pair=([A-Za-z0-9]+)/) || [])[1]}&hk=${(qr.match(/hk=([A-Za-z0-9_-]+)/) || [])[1]}`);
try {
  await phone.open();
  check("phone connects (trusted, the owner)", await phone.until(`!!window.__wado && window.__wado.e2eReady`, 20000));
  await phone.click(`document.querySelector(".ccard .cpick")`);
  check("the Computer page has a Clipboard section", await phone.until(`!!document.querySelector("#clipsec .clipsurf")`), logs.slice(-1500));

  // history
  deskCopy("hello from the desktop");
  check("a desktop copy appears in the history", await phone.until(`!!${tile("hello from the desktop")}`), logs.slice(-1500));
  check("…said whole to a screen reader, with where it came from",
    await phone.until(`(${tile("hello from the desktop")}?.querySelector(".sr")?.textContent || "").startsWith("Text: hello from the desktop. Copied on")`),
    await phone.ev(`${tile("hello from the desktop")}?.querySelector(".sr")?.textContent`));
  deskCopy("https://example.com/page");
  check("…a link as a link", await phone.until(`(${tile("example.com")} || {}).className?.includes("link")`));
  deskCopy(png, "image/png");
  check("…an image as an image, with its picture", await phone.until(`!!document.querySelector("#clipsec .cliptile.image .clippic img")`));

  // the switches: a phone trusted before pairing codes starts with both off
  check("a device paired before codes: sharing off by default",
    await phone.until(`${sw(0)}?.getAttribute("aria-checked") === "false" && ${sw(1)}?.getAttribute("aria-checked") === "false"`));
  await phone.ev(`navigator.clipboard.writeText("before the switch")`);
  deskCopy("not shared while off");
  await sleep(1500);
  check("…and a copy is not shared while it is off", (await phone.ev(phoneClip)) === "before the switch", await phone.ev(phoneClip));

  // computer → phone, by itself
  await phone.click(sw(0));
  check("switch on: Copy from this computer", await phone.until(`${sw(0)}.getAttribute("aria-checked") === "true"`));
  await sleep(500);
  deskCopy("auto to the phone");
  check("a desktop copy lands on the phone by itself", await phone.until(`${phoneClip}.then((t) => t === "auto to the phone")`, 10000), await phone.ev(phoneClip));
  check("…and is said in the live region", await phone.until(`${dock}.includes("Copied to this phone from")`), await phone.ev(dock));
  await sleep(300);
  await phone.shot("f10-said.png");

  // in the background: waits for focus
  await phone.ev(`(() => { window.__realFocus = document.hasFocus.bind(document); document.hasFocus = () => false; })()`);
  deskCopy("copied while away");
  await sleep(1500);
  check("copied while the page is away: not written yet", (await phone.ev(phoneClip)) === "auto to the phone", await phone.ev(phoneClip));
  await phone.ev(`(() => { document.hasFocus = window.__realFocus; window.dispatchEvent(new Event("focus")); })()`);
  check("…and written when it comes back", await phone.until(`${phoneClip}.then((t) => t === "copied while away")`, 6000), await phone.ev(phoneClip));

  // a browser that writes only inside a tap (Safari): the one-tap chip
  await phone.ev(`(() => { const c = navigator.clipboard; window.__realWrite = c.writeText.bind(c); c.writeText = () => Promise.reject(new DOMException("needs a tap", "NotAllowedError")); })()`);
  deskCopy("needs a tap");
  check("a refused write becomes a one-tap Copy chip", await phone.until(`!!document.querySelector("#clipdock .clipchip2.copy")`, 6000), await phone.ev(dock));
  await sleep(600);
  await phone.shot("f10-chip.png");
  await phone.ev(`navigator.clipboard.writeText = window.__realWrite`);
  await phone.click(`document.querySelector("#clipdock .chipgo")`);
  check("…and the tap puts it on the phone", await phone.until(`${phoneClip}.then((t) => t === "needs a tap")`, 6000), await phone.ev(phoneClip));
  check("…and the chip goes", await phone.until(`!document.querySelector("#clipdock .clipchip2")`));

  // phone → computer, on focus
  await phone.click(sw(1));
  check("switch on: Send this phone's copies", await phone.until(`${sw(1)}.getAttribute("aria-checked") === "true"`));
  await phone.ev(`navigator.clipboard.writeText("copied on the phone")`);
  await phone.ev(`window.dispatchEvent(new Event("focus"))`);
  check("the phone's copy goes to the computer when wado comes back", (await deskWait("copied on the phone")) === "copied on the phone");
  check("…said: Sent to", await phone.until(`${dock}.includes("Sent to")`), await phone.ev(dock));
  await sleep(1200);
  await phone.ev(`window.dispatchEvent(new Event("focus"))`);
  await sleep(1200);
  check("echo: it is not sent again, nor copied back", (await phone.ev(`window.__wado.clipShare._state.pending`)) === null
    && (await phone.ev(phoneClip)) === "copied on the phone");

  // secrets never travel
  execFileSync("wl-copy", ["--sensitive"], { env: D, input: "hunter2-secret", stdio: ["pipe", "ignore", "ignore"] });
  await sleep(2500);
  check("a password manager's copy never reaches the phone", (await phone.ev(phoneClip)) !== "hunter2-secret", await phone.ev(phoneClip));
  check("…nor its history", !(await phone.ev(`!!${tile("hunter2-secret")}`)));

  // pin
  await phone.click(`${tile("hello from the desktop")}.querySelector(".clipact.pin")`);
  check("pinned: it moves to the top", await phone.until(`(document.querySelector("#clipsec .cliptile") || {}).textContent?.includes("hello from the desktop")
    && document.querySelector("#clipsec .cliptile").classList.contains("pinned")`));
  deskCopy("newer than the pin");
  await phone.until(`!!${tile("newer than the pin")}`);
  check("…and stays above newer copies", await phone.ev(`document.querySelector("#clipsec .cliptile").textContent.includes("hello from the desktop")`));

  // search and kinds
  await phone.ev(`(() => { const i = document.querySelector("#clipsec .clipsearch input"); i.value = "example"; i.dispatchEvent(new Event("input", { bubbles: true })); })()`);
  check("search narrows the list", await phone.until(`JSON.stringify(${visible()}) === '["link"]'`), JSON.stringify(await phone.ev(visible())));
  await phone.ev(`(() => { const i = document.querySelector("#clipsec .clipsearch input"); i.value = ""; i.dispatchEvent(new Event("input", { bubbles: true })); })()`);
  await phone.click(`document.querySelector('#clipsec .clipkind[aria-label^="Pictures"]')`);
  check("the Pictures chip shows only pictures", await phone.until(`JSON.stringify(${visible()}) === '["image"]'`), JSON.stringify(await phone.ev(visible())));
  check("…and the chips count by kind", await phone.ev(`document.querySelector('#clipsec .clipkind[aria-label^="Links"]').getAttribute("aria-label") === "Links, 1"`));
  await phone.click(`document.querySelector('#clipsec .clipkind[aria-label^="All"]')`);
  await phone.until(`${visible()}.length >= 4`);

  // per entry: copy here, send there
  await phone.click(`${tile("example.com")}.querySelector(".clipact.go")`);
  check("Copy puts an entry on this phone", await phone.until(`${phoneClip}.then((t) => t === "https://example.com/page")`));
  check("…with a tick", await phone.until(`${tile("example.com")}.querySelector(".clipact.go").getAttribute("data-done") === "ok"`, 3000));
  await phone.click(`${tile("hello from the desktop")}.querySelector('[data-clip="send"]')`);
  check("Send puts an entry back on the computer's clipboard", (await deskWait("hello from the desktop")) === "hello from the desktop");

  // Send this phone's clipboard (one tap), text then a picture
  await phone.ev(`navigator.clipboard.writeText("typed on the phone")`);
  await phone.click(`document.querySelector("#clipsec .clipsend")`);
  check("\"Send this phone's clipboard\" puts it on the computer", (await deskWait("typed on the phone")) === "typed on the phone");
  check("…and it lands in the history", await phone.until(`!!${tile("typed on the phone")}`));
  await phone.ev(`fetch("data:image/png;base64,${png.toString("base64")}").then((r) => r.blob()).then((b) => navigator.clipboard.write([new ClipboardItem({ "image/png": b })]))`);
  await phone.click(`document.querySelector("#clipsec .clipsend")`);
  let types = "";
  for (let i = 0; i < 40 && !types.includes("image/png"); i++) { await sleep(250); try { types = deskPaste("--list-types"); } catch {} }
  check("…and a picture too", types.includes("image/png") && deskPaste("--type", "image/png").length > 0, types);

  // delete
  await phone.click(`${tile("newer than the pin")}.querySelector(".clipact.del")`);
  check("delete: the entry goes", await phone.until(`!${tile("newer than the pin")}`));
  check("…and so does cliphist's", !history().includes("newer than the pin"));
  await phone.ev(`document.querySelector("#clipsec").scrollIntoView({ block: "start" })`);
  await sleep(700);
  await phone.shot("f10-phone-computer-page.png");

  // keyboard: every action is a focusable, labelled button
  const unlabeled = await phone.ev(`[...document.querySelectorAll("#clipsec button")].filter((b) => !(b.getAttribute("aria-label") || b.textContent.trim())).length`);
  check("every clipboard button has a name", unlabeled === 0, unlabeled);
  const small = await phone.ev(`[...document.querySelectorAll("#clipsec .clipact, #clipsec .clipkind, #clipsec .sw, #clipsec .clipsend")].filter((b) => { const r = b.getBoundingClientRect(); return r.width && (r.width < 44 || r.height < 44) && !b.classList.contains("sw"); }).length`);
  check("targets are 44 px or more", small === 0, small);

  // the home widget: the same component, short
  await phone.click(`document.querySelector('.cbar .iconbtn[aria-label="Back to home"]')`);
  await phone.until(`!document.querySelector("#computer:not(.gone):not(.screen.gone)") || !!document.querySelector(".wempty")`, 4000);
  await sleep(800);
  await phone.click(`document.querySelector(".wempty")`);
  await phone.click(`[...document.querySelectorAll(".wadd")].find((b) => b.textContent.includes("Clipboard"))`);
  await phone.click(`document.querySelector("#wsect button")`);
  check("the Clipboard widget shows the newest three, pinned first",
    await phone.until(`(() => { const t = document.querySelectorAll(".widget .clipsurf.compact .cliptile"); return t.length === 3 && t[0].textContent.includes("hello from the desktop"); })()`, 8000),
    await phone.ev(`[...document.querySelectorAll(".widget .clipsurf.compact .cliptile")].map((t) => t.textContent.slice(0, 40))`));
  check("…with Copy and Send only", await phone.ev(`!document.querySelector(".widget .clipact.del") && !!document.querySelector(".widget .clipact.go")`));
  await phone.ev(`document.querySelector(".widget .clipsurf").scrollIntoView({ block: "center" })`);
  await sleep(700);
  await phone.shot("f10-widget.png");
  await phone.click(`document.querySelector(".widget .clipact.go")`);
  check("…and the widget's Copy works", await phone.until(`${phoneClip}.then((t) => t === "hello from the desktop")`));

  // grant, and the defaults of a device paired with a code
  await laptop.open();
  check("a QR-paired laptop connects", await laptop.until(`!!window.__wado && window.__wado.e2eReady`, 20000));
  // A wide screen opens with the console (its logs) up: close it, as a person would.
  for (let i = 0; i < 10; i++) {
    await sleep(500);
    await laptop.ev(`for (const e of document.querySelectorAll("#console.open .grab")) e.click(); true`);
  }
  for (let i = 0; i < 4 && !(await laptop.ev(`!!document.querySelector("#clipsec")`)); i++) {
    await laptop.click(`document.querySelector(".ccard .cpick")`);
    await laptop.until(`!!document.querySelector("#clipsec")`, 3000);
  }
  check("without the clipboard grant: what is missing, and the command",
    await laptop.until(`(document.querySelector("#clipsec .grant") || {}).textContent?.includes("wado allow")`), await laptop.ev(`document.querySelector("#clipsec")?.textContent`));
  execFileSync(join(BIN, "wado"), ["allow", "Laptop-key", "clipboard"], { env: { ...process.env, ...env } });
  await laptop.ev(`window.__wado.relaySendMsg({ type: "config_get" })`);
  await laptop.ev(`window.__wado.clipList()`);
  check("granted: the laptop sees the history", await laptop.until(`!!${tile("hello from the desktop")}`));
  check("a code-paired device with the grant: both switches on by default",
    await laptop.until(`${sw(0)}?.getAttribute("aria-checked") === "true" && ${sw(1)}?.getAttribute("aria-checked") === "true"`),
    await laptop.ev(`[...document.querySelectorAll("#clipsec .sw")].map((s) => s.getAttribute("aria-checked"))`));
  await sleep(800);
  deskCopy("to every paired device");
  check("…so a desktop copy reaches it with no tap at all", await laptop.until(`${phoneClip}.then((t) => t === "to every paired device")`, 10000), await laptop.ev(phoneClip));
  await laptop.ev(`document.querySelector("#clipsec").scrollIntoView({ block: "start" })`);
  await sleep(700);
  await laptop.shot("f10-laptop-computer-page.png");

  check("no script errors", !phone.errors.length && !laptop.errors.length, [...phone.errors, ...laptop.errors].join("\n"));
} finally {
  for (const p of procs) try { p.kill(); } catch {}
}
console.log(`\nscreenshots in ${SHOTS}`);
console.log(failures ? `${failures} FAILED` : "all passed");
process.exit(failures ? 1 : 0);
