// The file manager, end to end, in two real browsers against a pool of two daemons:
//
//   grant       a QR-paired device without a grant is refused, with the command that fixes it
//   scope       outside the root, a denied tree, a symlink out, `..`: all refused
//   download    bytes on disk in OPFS match the host's SHA-256; a paused one resumes and still does
//   zip         a folder arrives as a zip `unzip -t` accepts, the denied tree left out
//   upload      lands with the right bytes; a part left by a dropped upload is resumed; a name
//               clash asks, and "Keep both" keeps both; a folder keeps its tree
//   ops         new folder, rename, copy, move, trash (into the freedesktop Trash, with its info)
//   quick       pins are GTK bookmarks; recent files are listed; a PNG gets a thumbnail
//   access      an `ro` device reads but cannot write; devices can be granted from an rw device
//   toast       the other daemon's device is told about an upload
//   audit       every operation is in files.log
//   revoke      `wado files grant <device> none` applies to an open channel's next request
//   ui          the window opens in both layouts and lists the folder, no script errors
//
//   dx build -p wado-client --platform web && cargo build -p wado -p wado-relay
//   node scripts/files-e2e.mjs [target/debug]      (needs Chrome; run with the sandbox off)
import { spawn, execFileSync } from "node:child_process";
import { mkdtempSync, mkdirSync, writeFileSync, readFileSync, existsSync, symlinkSync, readdirSync } from "node:fs";
import { createHash, randomBytes } from "node:crypto";
import { deflateSync } from "node:zlib";
import { tmpdir } from "node:os";
import { join } from "node:path";

const ROOT = new URL("..", import.meta.url).pathname;
const BIN = join(ROOT, process.argv[2] || "target/debug");
const PUB = ROOT + "target/dx/wado-client/debug/web/public";
const PORT = 4997, RID = "999000777", HTTP = 8767;
const T = mkdtempSync(join(tmpdir(), "wado-files-e2e-"));
const HOME = join(T, "home");
const SHOTS = process.env.SHOTS || T;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const sha = (b) => createHash("sha256").update(b).digest("hex");
const procs = [];
let failures = 0;
const check = (name, ok, detail = "") => {
  if (!ok) failures++;
  console.log(`${ok ? "  ok  " : "FAIL  "}${name}${ok || !detail ? "" : "\n        " + detail}`);
};
for (const sig of ["SIGTERM", "SIGINT", "SIGHUP"]) process.on(sig, () => { for (const p of procs) try { p.kill(); } catch {} process.exit(1); });

// ── the computer's files ──────────────────────────────────────────────────────
const env = {
  HOME, XDG_CONFIG_HOME: join(HOME, ".config"), XDG_DATA_HOME: join(HOME, ".local/share"),
  XDG_STATE_HOME: join(HOME, ".local/state"), XDG_CACHE_HOME: join(HOME, ".cache"),
};
for (const d of ["docs/deep", "Downloads", "Pictures", ".ssh", ".config/wado", "outside"]) mkdirSync(join(HOME, d), { recursive: true });
mkdirSync(join(T, "elsewhere"));
writeFileSync(join(T, "elsewhere/secret.txt"), "not yours");
writeFileSync(join(HOME, ".ssh/id_ed25519"), "PRIVATE KEY");
writeFileSync(join(HOME, "docs/a.txt"), "hello from the computer\n");
writeFileSync(join(HOME, "docs/deep/b.txt"), "deeper\n");
const big = randomBytes(24 << 20);
writeFileSync(join(HOME, "Downloads/big.bin"), big);
const huge = randomBytes(160 << 20);
writeFileSync(join(HOME, "Downloads/huge.bin"), huge);
symlinkSync(join(T, "elsewhere"), join(HOME, "escape"));
// A real PNG, 64×64, for the thumbnailer.
const png = (() => {
  const crc = (b) => { let c = ~0; for (const x of b) { c ^= x; for (let k = 0; k < 8; k++) c = c & 1 ? (c >>> 1) ^ 0xedb88320 : c >>> 1; } return ~c >>> 0; };
  const chunk = (t, d) => { const l = Buffer.alloc(4); l.writeUInt32BE(d.length); const td = Buffer.concat([Buffer.from(t), d]); const c = Buffer.alloc(4); c.writeUInt32BE(crc(td)); return Buffer.concat([l, td, c]); };
  const ihdr = Buffer.alloc(13); ihdr.writeUInt32BE(64, 0); ihdr.writeUInt32BE(64, 4); ihdr[8] = 8; ihdr[9] = 2;
  const raw = Buffer.alloc(64 * (1 + 64 * 3)); for (let y = 0; y < 64; y++) for (let x = 0; x < 64; x++) raw.set([x * 4, y * 4, 128], y * 193 + 1 + x * 3);
  return Buffer.concat([Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]), chunk("IHDR", ihdr), chunk("IDAT", deflateSync(raw)), chunk("IEND", Buffer.alloc(0))]);
})();
writeFileSync(join(HOME, "Pictures/p.png"), png);
writeFileSync(join(HOME, ".config/wado/config.kdl"), `files { hidden #true; deny "~/outside"; }\n`);
writeFileSync(join(HOME, ".config/wado/trusted_clients"), "");

let daemonLog = "";
const start = (cmd, args, e = {}) => {
  const p = spawn(cmd, args, { env: { ...process.env, ...env, ...e }, stdio: ["ignore", "pipe", "pipe"] });
  for (const s of [p.stdout, p.stderr]) s.on("data", (d) => { daemonLog += d.toString().replace(/\x1b\[[0-9;]*m/g, ""); if (process.env.E2E_LOG) process.stdout.write(d); });
  procs.push(p);
  return p;
};
const wado = (...args) => execFileSync(join(BIN, "wado"), args, { env: { ...process.env, ...env } }).toString();

start(join(BIN, "wado-relay"), ["--bind", "127.0.0.1:" + PORT]);
await sleep(400);
for (const n of ["e2e-files-a", "e2e-files-b"]) {
  start(join(BIN, "wado"), [], { WADO_RELAY_URL: "ws://127.0.0.1:" + PORT, WADO_REMOTE_ID: RID, WADO_INSTANCE: n, WADO_UDP_SLICE: n.endsWith("a") ? "2" : "3" });
  await sleep(300);
}
start("python3", ["-m", "http.server", String(HTTP), "--bind", "127.0.0.1", "-d", PUB]);
await sleep(800);

class Browser {
  constructor(name, port, key, mobile, extra) { Object.assign(this, { name, port, key, mobile, extra, id: 0, pend: new Map(), errors: [] }); }
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
    await this.cdp("Emulation.setDeviceMetricsOverride", this.mobile
      ? { width: 412, height: 915, deviceScaleFactor: 2, mobile: true } : { width: 1440, height: 900, deviceScaleFactor: 1, mobile: false });
    if (this.mobile) await this.cdp("Emulation.setTouchEmulationEnabled", { enabled: true, maxTouchPoints: 5 });
    await this.cdp("Page.addScriptToEvaluateOnNewDocument", { source: `try { localStorage.setItem("wado.client", ${JSON.stringify(this.key)}); } catch (_) {}` });
    await this.cdp("Page.navigate", { url: `http://127.0.0.1:${HTTP}/?relay=http://127.0.0.1:${PORT}&id=${RID}${this.extra}` });
  }
  cdp(method, params = {}) { return new Promise((r) => { const i = ++this.id; this.pend.set(i, r); this.ws.send(JSON.stringify({ id: i, method, params })); }); }
  async ev(expr) {
    const r = await this.cdp("Runtime.evaluate", { expression: expr, returnByValue: true, awaitPromise: true });
    if (r.result?.exceptionDetails) return { thrown: r.result.exceptionDetails.exception?.description || "threw" };
    return r.result?.result?.value;
  }
  async until(expr, ms = 15000) { const t = Date.now(); while (Date.now() - t < ms) { if (await this.ev(expr)) return true; await sleep(250); } return false; }
  // A request on the files channel: `{ok…}` or `{err}`.
  req(op, args = {}) {
    return this.ev(`window.__wado.files.req(${JSON.stringify(op)}, ${JSON.stringify(args)}).catch((e) => ({ err: String(e.message || e) }))`);
  }
  async shot(file) {
    const r = await this.cdp("Page.captureScreenshot", { format: "png" });
    if (r.result?.data) writeFileSync(join(SHOTS, file), Buffer.from(r.result.data, "base64"));
  }
}
const qrLink = () => wado("qr", "--relay", "http://x", "--id", RID);
const params = (link) => {
  const pair = (link.match(/pair=([A-Za-z0-9]+)/) || [])[1];
  const hk = (link.match(/hk=([A-Za-z0-9_-]+)/) || [])[1];
  return `&pair=${pair}&hk=${hk}`;
};
const xferDone = (b, name, ms = 60000) => b.until(`(window.__wado.files.xfers.find((x) => x.name === ${JSON.stringify(name)}) || {}).state === "done"`, ms);
const xferOf = (b, name) => b.ev(`(({ state, err, done, size }) => ({ state, err, done, size }))(window.__wado.files.xfers.find((x) => x.name === ${JSON.stringify(name)}) || {})`);
// The bytes of a finished download, from OPFS, hashed in the page.
const opfsSha = (b, name) => b.ev(`(async () => {
  const x = window.__wado.files.xfers.find((x) => x.name === ${JSON.stringify(name)});
  const dir = await (await navigator.storage.getDirectory()).getDirectoryHandle("wado-downloads");
  const f = await (await dir.getFileHandle(x.opfs)).getFile();
  return [...new Uint8Array(await crypto.subtle.digest("SHA-256", await f.arrayBuffer()))].map((b) => b.toString(16).padStart(2, "0")).join("");
})()`);
const H = (p) => join(HOME, p);

const phone = new Browser("phone", 9351, "Phone-key", true, params(qrLink()));
const laptop = new Browser("laptop", 9352, "Laptop-key", false, params(qrLink()));
try {
  await phone.open();
  check("phone joins with the QR's code", await phone.until(`!!window.__wado && window.__wado.e2eReady`, 20000));
  check("…and is pinned in the trust list", /Phone-key\t[^\n]*\t1(\t|\n|$)/.test(readFileSync(H(".config/wado/trusted_clients"), "utf8")));

  // grant
  const refused = await phone.ev(`window.__wado.files.connect().then(() => "opened", (e) => String(e.message || e))`);
  check("no grant: refused, and told the command", /wado files grant/.test(refused), refused);
  check("wado files grant", /Phone.*: files rw|files rw/.test(wado("files", "grant", "Phone-key", "rw")));
  const hello = await phone.ev(`window.__wado.files.connect().then((i) => i, (e) => ({ err: String(e.message || e) }))`);
  check("granted: the channel opens with rw", hello && hello.access === "rw", JSON.stringify(hello));

  // scope
  const home = await phone.req("list", { path: HOME });
  const names = (home.entries || []).map((e) => e.name);
  check("home lists its folders", names.includes("docs") && names.includes("Downloads"), JSON.stringify(names));
  check("denied trees are not listed (hidden on)", !names.includes(".ssh") && !names.includes("outside") && names.includes(".config"), JSON.stringify(names));
  const cfg = await phone.req("list", { path: H(".config") });
  check("~/.config/wado is never listed", !(cfg.entries || []).some((e) => e.name === "wado"), JSON.stringify(cfg));
  for (const [what, args] of [
    ["outside the root", { path: "/etc" }],
    ["a denied tree", { path: H(".ssh") }],
    ["a configured deny", { path: H("outside") }],
    ["a symlink out", { path: H("escape") }],
    ["`..`", { path: H("docs/../..") }],
    ["~/.config/wado", { path: H(".config/wado") }],
  ]) {
    const r = await phone.req("list", args);
    check("refused: " + what, !!r.err, JSON.stringify(r).slice(0, 200));
  }
  const key = await phone.req("get", { path: H(".ssh/id_ed25519") });
  check("refused: downloading a key", !!key.err, JSON.stringify(key));

  // download
  await phone.ev(`window.__wado.files.download(${JSON.stringify(H("Downloads/big.bin"))}, "big.bin", false); true`);
  check("download finishes", await xferDone(phone, "big.bin"), JSON.stringify(await xferOf(phone, "big.bin")));
  check("…and its bytes match the computer's", (await opfsSha(phone, "big.bin")) === sha(big));
  await phone.ev(`window.__wado.files.download(${JSON.stringify(H("Downloads/huge.bin"))}, "huge.bin", false); true`);
  await phone.until(`(window.__wado.files.xfers.find((x) => x.name === "huge.bin") || {}).done > (8 << 20)`, 30000);
  await phone.ev(`window.__wado.files.pause(window.__wado.files.xfers.find((x) => x.name === "huge.bin")).then(() => true)`);
  const paused = await xferOf(phone, "huge.bin");
  check("a download pauses part-way", paused.state === "paused" && paused.done > 0 && paused.done < huge.length, JSON.stringify(paused));
  await sleep(500);
  await phone.ev(`window.__wado.files.resume(window.__wado.files.xfers.find((x) => x.name === "huge.bin")); true`);
  check("…resumes and finishes", await xferDone(phone, "huge.bin", 120000), JSON.stringify(await xferOf(phone, "huge.bin")));
  check("…with the whole file's hash right", (await opfsSha(phone, "huge.bin")) === sha(huge));

  // zip
  writeFileSync(H("docs/.hidden"), "dot");
  await phone.ev(`window.__wado.files.download(${JSON.stringify(H("docs"))}, "docs", true); true`);
  check("a folder downloads as a zip", await xferDone(phone, "docs.zip"), JSON.stringify(await xferOf(phone, "docs.zip")));
  const zipB64 = await phone.ev(`(async () => {
    const x = window.__wado.files.xfers.find((x) => x.name === "docs.zip");
    const dir = await (await navigator.storage.getDirectory()).getDirectoryHandle("wado-downloads");
    const b = new Uint8Array(await (await (await dir.getFileHandle(x.opfs)).getFile()).arrayBuffer());
    let s = ""; for (const c of b) s += String.fromCharCode(c); return btoa(s); })()`);
  writeFileSync(join(T, "docs.zip"), Buffer.from(zipB64 || "", "base64"));
  let unzip = "";
  try { unzip = execFileSync("python3", ["-c", "import zipfile,sys; z=zipfile.ZipFile(sys.argv[1]); assert z.testzip() is None; print(sorted(z.namelist())); print(z.read('docs/deep/b.txt').decode())", join(T, "docs.zip")]).toString(); } catch (e) { unzip = String(e.stdout || e); }
  check("…that a zip reader accepts, with the tree and the bytes", unzip.includes("docs/deep/b.txt") && unzip.includes("docs/.hidden") && unzip.includes("deeper"), unzip);

  // upload
  const up = await phone.ev(`(async () => {
    const bytes = new Uint8Array(3 << 20); for (let i = 0; i < bytes.length; i += 65536) crypto.getRandomValues(bytes.subarray(i, i + 65536));
    window.__wado.files.upload(${JSON.stringify(H("docs"))}, [new File([bytes], "up.bin")]);
    return [...new Uint8Array(await crypto.subtle.digest("SHA-256", bytes))].map((b) => b.toString(16).padStart(2, "0")).join(""); })()`);
  check("upload finishes", await xferDone(phone, "up.bin"), JSON.stringify(await xferOf(phone, "up.bin")));
  check("…with the right bytes on the computer", existsSync(H("docs/up.bin")) && sha(readFileSync(H("docs/up.bin"))) === up);
  check("…and no part left behind", !existsSync(H("docs/up.bin.wado-part")));
  // A dropped upload's part: the next attempt carries on from it.
  const resumeBytes = randomBytes(2 << 20);
  writeFileSync(H("docs/resumed.bin.wado-part"), resumeBytes.subarray(0, 1 << 20));
  await phone.ev(`(() => { const b = Uint8Array.from(atob(${JSON.stringify(resumeBytes.toString("base64"))}), (c) => c.charCodeAt(0));
    window.__wado.files.upload(${JSON.stringify(H("docs"))}, [new File([b], "resumed.bin")]); return true; })()`);
  check("an upload resumes from its part", await xferDone(phone, "resumed.bin") && sha(readFileSync(H("docs/resumed.bin"))) === sha(resumeBytes));
  // A clash asks; "Keep both" keeps both.
  await phone.ev(`window.__wado.files.upload(${JSON.stringify(H("docs"))}, [new File(["new a"], "a.txt")]); true`);
  check("a name clash asks", await phone.until(`!!document.querySelector(".wado-prompt") && document.querySelector(".wado-prompt").textContent.includes("already exists")`));
  await phone.ev(`[...document.querySelectorAll(".wado-prompt .menurow")].find((b) => b.textContent === "Keep both").click(); true`);
  await sleep(1500);
  check("…Keep both keeps both", readFileSync(H("docs/a.txt"), "utf8").startsWith("hello") && existsSync(H("docs/a (1).txt")) && readFileSync(H("docs/a (1).txt"), "utf8") === "new a");
  await phone.ev(`(() => { const f = new File(["leaf"], "leaf.txt"); f.wadoPath = "tree/sub/leaf.txt"; window.__wado.files.upload(${JSON.stringify(H("docs"))}, [f]); return true; })()`);
  await sleep(1500);
  check("a folder upload keeps its tree", existsSync(H("docs/tree/sub/leaf.txt")));
  const bad = await phone.ev(`(async () => { const F = window.__wado.files;
    return await new Promise((ok) => F.stream("put", { dir: ${JSON.stringify(H("docs"))}, name: "../../evil", size: 1 }, { msg: (m) => { ok(m); return true; } })); })()`);
  check("refused: an upload named ../", !!bad.err && !existsSync(join(T, "evil")), JSON.stringify(bad));

  // ops
  check("new folder", !(await phone.req("mkdir", { path: H("docs/made") })).err && existsSync(H("docs/made")));
  check("rename", !(await phone.req("rename", { path: H("docs/made"), to: "renamed" })).err && existsSync(H("docs/renamed")));
  check("copy", !(await phone.req("copy", { paths: [H("docs/a.txt")], dest: H("docs/renamed") })).err && existsSync(H("docs/renamed/a.txt")));
  check("move", !(await phone.req("move", { paths: [H("docs/renamed")], dest: H("Downloads") })).err && existsSync(H("Downloads/renamed/a.txt")) && !existsSync(H("docs/renamed")));
  check("move into a denied tree is refused", !!(await phone.req("move", { paths: [H("docs/a (1).txt")], dest: H(".ssh") })).err);
  check("moving a tree that holds a denied one is refused", !!(await phone.req("move", { paths: [H(".config")], dest: H("docs") })).err);
  check("trash", !(await phone.req("trash", { paths: [H("Downloads/renamed")] })).err && !existsSync(H("Downloads/renamed"))
    && existsSync(H(".local/share/Trash/files/renamed")) && readFileSync(H(".local/share/Trash/info/renamed.trashinfo"), "utf8").includes("Path=" + H("Downloads/renamed")));

  // quick
  check("pin", !(await phone.req("pin", { path: H("docs") })).err && readFileSync(H(".config/gtk-3.0/bookmarks"), "utf8").includes("file://" + H("docs")));
  const quick = await phone.req("quick");
  check("pins and XDG folders are offered", (quick.pins || []).some((p) => p.path === H("docs") && p.kind === "pin") && (quick.pins || []).some((p) => p.path === H("Downloads")), JSON.stringify(quick.pins));
  check("recent files are listed", (quick.recent || []).some((r) => r.name === "big.bin"), JSON.stringify((quick.recent || []).map((r) => r.name)));
  check("unpin", !(await phone.req("unpin", { path: H("docs") })).err && !readFileSync(H(".config/gtk-3.0/bookmarks"), "utf8").includes(H("docs")));
  const th = await phone.req("thumb", { path: H("Pictures/p.png") });
  check("a PNG gets a thumbnail", !!th.png && Buffer.from(th.png, "base64").subarray(1, 4).toString() === "PNG", JSON.stringify(th).slice(0, 200));

  // access: the laptop, ro, granted from the phone
  await laptop.open();
  check("laptop joins with the QR's code", await laptop.until(`!!window.__wado && window.__wado.e2eReady`, 20000));
  const devs = await phone.req("devices");
  check("devices are listed for an rw device", (devs.devices || []).length === 2, JSON.stringify(devs));
  const lkey = (devs.devices || []).find((d) => !d.me)?.key;
  check("grant from the phone", !(await phone.req("grant", { key: lkey, level: "ro" })).err);
  await laptop.ev(`(() => { const W = window.__wado; W._notes = []; const prev = W._relayHandlers.files_note; W.relayOn("files_note", (m) => { W._notes.push(m); prev(m); }); return true; })()`);
  const lhello = await laptop.ev(`window.__wado.files.connect().then((i) => i, (e) => ({ err: String(e.message || e) }))`);
  check("the laptop opens files read-only", lhello && lhello.access === "ro", JSON.stringify(lhello));
  check("ro: may list", !(await laptop.req("list", { path: H("docs") })).err);
  const ro = await laptop.req("mkdir", { path: H("docs/nope") });
  check("ro: may not write", /read/.test(ro.err || "") && !existsSync(H("docs/nope")), JSON.stringify(ro));
  check("ro: may not grant", !!(await laptop.req("grant", { key: "Laptop-key", level: "rw" })).err);

  // toast: the phone (other daemon) uploads, the laptop hears of it
  check("phone and laptop are on different daemons", (await phone.ev(`(window.__wado.pool || {}).instance`)) !== (await laptop.ev(`(window.__wado.pool || {}).instance`)));
  await phone.ev(`window.__wado.files.upload(${JSON.stringify(H("docs"))}, [new File(["toast"], "toast.txt")]); true`);
  check("the other device gets a toast", await laptop.until(`window.__wado._notes.some((m) => m.op === "upload" && m.path.endsWith("toast.txt"))`, 10000));

  // audit
  const log = readFileSync(H(".local/state/wado/files.log"), "utf8");
  check("operations are in files.log", ["upload", "download", "mkdir", "rename", "copy", "move", "trash", "grant"].every((op) => log.includes("\t" + op + "\t")),
    log.split("\n").slice(-5).join(" | "));

  // ui
  await phone.ev(`window.__wado.filesOpen(); true`);
  check("phone: the window opens on its home", await phone.until(`!!document.querySelector('#wado-files[data-layout="narrow"] .fchips')`));
  await phone.ev(`[...document.querySelectorAll("#wado-files .fplace")].find((b) => b.textContent.includes("Home")).click(); true`);
  check("phone: a folder lists", await phone.until(`[...document.querySelectorAll("#wado-files .frow .fname")].some((n) => n.textContent.startsWith("docs"))`));
  await phone.shot("files-phone.png");
  await phone.ev(`window.__wado.files.showTransfers(true); true`);
  await sleep(500);
  await phone.shot("files-phone-transfers.png");
  await laptop.ev(`window.__wado.filesOpen(); true`);
  check("laptop: the window opens wide, with a sidebar", await laptop.until(`!!document.querySelector('#wado-files[data-layout="wide"] .fside') && document.querySelectorAll("#wado-files .frow").length > 3`));
  await laptop.ev(`window.__wado.files.ui.grid = true; window.__wado.files.render(); true`);
  await sleep(300);
  await laptop.shot("files-laptop-grid.png");
  await laptop.ev(`window.__wado.files.ui.grid = false; window.__wado.files.render(); true`);
  await laptop.ev(`(() => { const r = [...document.querySelectorAll("#wado-files .frow")].find((x) => x.textContent.includes("Pictures")); r.dispatchEvent(new MouseEvent("dblclick", { bubbles: true })); return true; })()`);
  await sleep(1200);
  await laptop.shot("files-laptop.png");
  check("no script errors", !phone.errors.length && !laptop.errors.length, [...phone.errors, ...laptop.errors].join("\n        "));

  // revoke
  wado("files", "grant", "Phone-key", "none");
  const gone = await phone.req("list", { path: HOME });
  check("revoked: the open channel's next request is refused", /no longer has file access/.test(gone.err || ""), JSON.stringify(gone).slice(0, 200));
} catch (e) {
  failures++;
  console.log("FAIL  threw: " + (e.stack || e));
} finally {
  if (failures) console.log(daemonLog.split("\n").filter((l) => /files|WARN|ERROR/.test(l)).slice(-40).join("\n"));
  for (const p of procs) try { p.kill(); } catch {}
  console.log(failures ? `\n${failures} check(s) failed` : "\nall files checks passed");
  console.log("screenshots in " + SHOTS);
  process.exit(failures ? 1 : 0);
}
