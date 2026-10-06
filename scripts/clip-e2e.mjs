// The computer's clipboard rail, end to end, in two real browsers.
//
// "The computer's desktop" is a second wado daemon's Wayland socket: its compositor speaks
// data-control, so `wl-copy`, `wl-paste --watch cliphist store` and cliphist work against it
// exactly as on a real desktop — and the person's own clipboard is never touched.
//
//   history     a copy on the desktop appears on the rail, as text, link or image (with picture)
//   pin         a pinned tile moves to the top and stays there
//   search      the search box and the kind chips narrow the list
//   copy        tapping a tile puts it on this device's clipboard
//   send        "Send mine" puts this device's clipboard (text, then an image) on the desktop
//   delete      the tile goes, and so does cliphist's entry
//   grant       a device without the `clipboard` grant is refused, and told the command
//   ui          docked rail on a wide screen, drawer on a phone; no script errors
//
//   dx build -p wado-client --platform web && cargo build --release -p wado -p wado-relay
//   node scripts/clip-e2e.mjs [target/release]      (needs Chrome, wl-clipboard, cliphist; sandbox off)
import { spawn, execFileSync } from "node:child_process";
import { mkdtempSync, mkdirSync, writeFileSync } from "node:fs";
import { deflateSync } from "node:zlib";
import { tmpdir } from "node:os";
import { join } from "node:path";

const ROOT = new URL("..", import.meta.url).pathname;
const BIN = join(ROOT, process.argv[2] || "target/release");
const PUB = ROOT + "target/dx/wado-client/debug/web/public";
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
writeFileSync(join(HOME, ".config/wado/trusted_clients"), "");

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

const tile = (text) => `[...document.querySelectorAll("#cliprail .cliptile")].find((t) => t.textContent.includes(${JSON.stringify(text)}))`;
const visible = () => `[...document.querySelectorAll("#cliprail .cliptile:not(.leaving)")].map((t) => t.className.split(" ")[1])`;
// A 48×48 PNG, made here.
const png = (() => {
  const crc = (b) => { let c = ~0; for (const x of b) { c ^= x; for (let k = 0; k < 8; k++) c = c & 1 ? (c >>> 1) ^ 0xedb88320 : c >>> 1; } return ~c >>> 0; };
  const chunk = (t, d) => { const l = Buffer.alloc(4); l.writeUInt32BE(d.length); const td = Buffer.concat([Buffer.from(t), d]); const c = Buffer.alloc(4); c.writeUInt32BE(crc(td)); return Buffer.concat([l, td, c]); };
  const ihdr = Buffer.alloc(13); ihdr.writeUInt32BE(48, 0); ihdr.writeUInt32BE(48, 4); ihdr[8] = 8; ihdr[9] = 2;
  const raw = Buffer.alloc(48 * (1 + 48 * 3)); for (let y = 0; y < 48; y++) for (let x = 0; x < 48; x++) raw.set([x * 5, y * 5, 160], y * 145 + 1 + x * 3);
  return Buffer.concat([Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]), chunk("IHDR", ihdr), chunk("IDAT", deflateSync(raw)), chunk("IEND", Buffer.alloc(0))]);
})();

const phone = new Browser("phone", 9361, "Phone-key", true);
const qr = execFileSync(join(BIN, "wado"), ["qr", "--relay", "http://x", "--id", RID], { env: { ...process.env, ...env } }).toString();
const laptop = new Browser("laptop", 9362, "Laptop-key", false,
  `&pair=${(qr.match(/pair=([A-Za-z0-9]+)/) || [])[1]}&hk=${(qr.match(/hk=([A-Za-z0-9_-]+)/) || [])[1]}`);
try {
  await phone.open();
  check("phone connects (first device: the owner)", await phone.until(`!!window.__wado && window.__wado.e2eReady`, 20000));

  // history
  deskCopy("hello from the desktop");
  check("a desktop copy appears on the rail", await phone.until(`!!${tile("hello from the desktop")}`), logs.slice(-1500));
  deskCopy("https://example.com/page");
  check("…a link as a link", await phone.until(`(${tile("example.com")} || {}).className?.includes("link")`));
  deskCopy(png, "image/png");
  check("…an image as an image, with its picture", await phone.until(`!!document.querySelector("#cliprail .cliptile.image .clippic img")`));

  // the drawer
  check("on a phone the rail starts hidden behind its tab", await phone.ev(`!document.querySelector("#cliprail").classList.contains("open")`));
  await phone.click(`document.querySelector(".cliptab")`);
  check("…and the tab opens it", await phone.until(`document.querySelector("#cliprail").classList.contains("open")`));
  await sleep(700);
  await phone.shot("clip-phone.png");

  // pin
  await phone.click(`${tile("hello from the desktop")}.querySelector(".clipbtn")`);
  check("pinned: it moves to the top", await phone.until(`(document.querySelector("#cliprail .cliptile") || {}).textContent?.includes("hello from the desktop")
    && document.querySelector("#cliprail .cliptile").classList.contains("pinned")`));
  deskCopy("newer than the pin");
  await phone.until(`!!${tile("newer than the pin")}`);
  check("…and stays above newer copies", await phone.ev(`document.querySelector("#cliprail .cliptile").textContent.includes("hello from the desktop")`));

  // search and chips
  await phone.ev(`(() => { const i = document.querySelector("#cliprail .clipsearch input"); i.value = "example"; i.dispatchEvent(new Event("input", { bubbles: true })); })()`);
  check("search narrows the list", await phone.until(`JSON.stringify(${visible()}) === '["link"]'`), JSON.stringify(await phone.ev(visible())));
  await phone.ev(`(() => { const i = document.querySelector("#cliprail .clipsearch input"); i.value = ""; i.dispatchEvent(new Event("input", { bubbles: true })); })()`);
  await phone.click(`document.querySelector('#cliprail .clipchip[title="Images"]')`);
  check("the Images chip shows only images", await phone.until(`JSON.stringify(${visible()}) === '["image"]'`), JSON.stringify(await phone.ev(visible())));
  check("…and the chips count by kind", await phone.ev(`document.querySelector('#cliprail .clipchip[title="Links"] span').textContent === "1"`));
  await phone.click(`document.querySelector('#cliprail .clipchip[title="All"]')`);
  await phone.until(`${visible()}.length >= 4`);

  // copy to this device
  await phone.click(tile("example.com"));
  check("tapping a tile copies it to this device", await phone.until(`navigator.clipboard.readText().then((t) => t === "https://example.com/page")`));

  // send mine
  await phone.ev(`navigator.clipboard.writeText("typed on the phone")`);
  await phone.click(`document.querySelector("#cliprail .clippaste")`);
  let got = "";
  for (let i = 0; i < 40 && got !== "typed on the phone"; i++) { await sleep(250); try { got = deskPaste("--no-newline"); } catch {} }
  check("\"Send mine\" puts this device's text on the desktop", got === "typed on the phone", got);
  check("…and it lands in the rail", await phone.until(`!!${tile("typed on the phone")}`));
  await phone.ev(`fetch("data:image/png;base64,${png.toString("base64")}").then((r) => r.blob()).then((b) => navigator.clipboard.write([new ClipboardItem({ "image/png": b })]))`);
  await phone.click(`document.querySelector("#cliprail .clippaste")`);
  let types = "";
  for (let i = 0; i < 40 && !types.includes("image/png"); i++) { await sleep(250); try { types = deskPaste("--list-types"); } catch {} }
  check("…and an image too", types.includes("image/png") && deskPaste("--type", "image/png").length > 0, types);

  // delete
  await phone.click(`${tile("newer than the pin")}.querySelectorAll(".clipbtn")[1]`);
  check("delete: the tile goes", await phone.until(`!${tile("newer than the pin")}`),
    JSON.stringify(await phone.ev(`[${tile("newer than the pin")}?.className, [...document.querySelectorAll(".toast, .note, [class*=toast]")].map((e) => e.textContent).join("|")]`)));
  check("…and so does cliphist's entry", !history().includes("newer than the pin"));
  await phone.shot("clip-phone-after.png");

  // grant
  await laptop.open();
  check("a QR-paired laptop connects", await laptop.until(`!!window.__wado && window.__wado.e2eReady`, 20000));
  check("without the clipboard grant it is refused, and told the command",
    await laptop.until(`(document.querySelector("#cliprail .clipnote") || {}).textContent?.includes("wado allow")`));
  execFileSync(join(BIN, "wado"), ["allow", "Laptop-key", "clipboard"], { env: { ...process.env, ...env } });
  await laptop.ev(`window.__wado.clipList()`);
  check("granted: the laptop sees the history", await laptop.until(`!!${tile("hello from the desktop")}`));
  check("on a wide screen the rail is docked, no tab", await laptop.ev(`getComputedStyle(document.querySelector(".cliptab")).display === "none"`));
  await sleep(700);
  await laptop.shot("clip-laptop.png");

  check("no script errors", !phone.errors.length && !laptop.errors.length, [...phone.errors, ...laptop.errors].join("\n"));
} finally {
  for (const p of procs) try { p.kill(); } catch {}
}
console.log(`\nscreenshots in ${SHOTS}`);
console.log(failures ? `${failures} FAILED` : "all passed");
process.exit(failures ? 1 : 0);
