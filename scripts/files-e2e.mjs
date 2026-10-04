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
//   viewer      images, a gallery, text/JSON, audio; HTML shown as source and never run
//   streaming   MP4 played directly and MKV converted, as they arrive; seek; captions from a file
//               beside it and from inside it; audio tracks; VLC keys; photo zoom; video
//               thumbnails; ffmpeg runs at nice 19 and stops when the player closes
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
writeFileSync(join(HOME, "Pictures/q.png"), png);
// A camera-sized photo, for fitting and zooming on a phone.
execFileSync("ffmpeg", ["-hide_banner", "-loglevel", "error", "-y", "-f", "lavfi", "-i", "testsrc2=size=4000x3000", "-frames:v", "1", join(HOME, "Pictures/zbig.jpg")]);
writeFileSync(join(HOME, "Pictures/page.html"), "<script>window.__pwned = 1</script><b>hi</b>");
writeFileSync(join(HOME, "Pictures/notes.json"), '{"a":1,"b":[2,3]}');
// One second of a 440 Hz tone, 8 kHz mono 16-bit WAV.
writeFileSync(join(HOME, "Pictures/tone.wav"), (() => {
  const n = 8000, b = Buffer.alloc(44 + n * 2);
  b.write("RIFF", 0); b.writeUInt32LE(36 + n * 2, 4); b.write("WAVEfmt ", 8); b.writeUInt32LE(16, 16); b.writeUInt16LE(1, 20);
  b.writeUInt16LE(1, 22); b.writeUInt32LE(8000, 24); b.writeUInt32LE(16000, 28); b.writeUInt16LE(2, 32); b.writeUInt16LE(16, 34);
  b.write("data", 36); b.writeUInt32LE(n * 2, 40);
  for (let i = 0; i < n; i++) b.writeInt16LE(Math.round(8000 * Math.sin((2 * Math.PI * 440 * i) / 8000)), 44 + i * 2);
  return b; })());
// Media, made by ffmpeg: H.264/AAC MP4 (played directly) with a subtitle file beside it, and an
// MKV of MPEG-4 Part 2 + MP3 with two audio tracks and an embedded subtitle (converted).
mkdirSync(join(HOME, "Videos"));
writeFileSync(join(T, "s.srt"), "1\n00:00:01,000 --> 00:00:30,000\nembedded words\n");
writeFileSync(join(HOME, "Videos/film.en.srt"), "1\n00:00:00,500 --> 00:00:40,000\nhello from the side file\n");
const ff = (...a) => execFileSync("ffmpeg", ["-hide_banner", "-loglevel", "error", "-y", ...a]);
ff("-f", "lavfi", "-i", "testsrc2=size=640x360:rate=30", "-f", "lavfi", "-i", "sine=frequency=440", "-t", "40",
  "-c:v", "libx264", "-profile:v", "high", "-pix_fmt", "yuv420p", "-g", "30", "-c:a", "aac", join(HOME, "Videos/film.mp4"));
ff("-f", "lavfi", "-i", "testsrc=size=640x360:rate=25", "-f", "lavfi", "-i", "sine=frequency=330", "-f", "lavfi", "-i", "sine=frequency=550",
  "-i", join(T, "s.srt"), "-t", "300", "-map", "0", "-map", "1", "-map", "2", "-map", "3", "-c:v", "mpeg4", "-q:v", "5",
  "-c:a", "libmp3lame", "-c:s", "srt", "-metadata:s:a:0", "language=eng", "-metadata:s:a:1", "language=fra", join(HOME, "Videos/other.mkv"));
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
      "--user-data-dir=" + join(T, "chrome-" + this.name), "--no-first-run", "--autoplay-policy=no-user-gesture-required", "about:blank"]);
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
  // the revision: grid in the narrow layout, search, categories, storage, trash restore, properties
  await phone.ev(`document.querySelector("#wado-files .fviewbtn").click(); true`);
  check("phone: one tap switches to the grid", await phone.until(`document.querySelectorAll("#wado-files .fgrid .ftile").length > 3`));
  await sleep(1200);
  await phone.shot("files-phone-grid.png");
  await phone.ev(`document.querySelector("#wado-files .fviewbtn").click(); true`);
  check("phone: …and back to the list", await phone.until(`document.querySelectorAll("#wado-files .flist .frow").length > 3`));
  await phone.ev(`document.querySelector("#wado-files .fhrow .fbtn").click(); true`);
  check("phone: back from Home is the overview, with categories and storage", await phone.until(`!!document.querySelector("#wado-files .fcats") && !!document.querySelector("#wado-files .fstore")`));
  await phone.shot("files-phone-home.png");
  await phone.ev(`[...document.querySelectorAll("#wado-files .fcat")].find((b) => b.textContent.includes("Photos")).click(); true`);
  check("category: Photos finds the pictures", await phone.until(`[...document.querySelectorAll("#wado-files .ftile .fname")].some((n) => n.textContent.startsWith("p.png"))`, 20000));
  const found = await phone.req("find", { path: HOME, query: "BIG" });
  check("search: by name, any case, in subfolders", (found.entries || []).some((e) => e.name === "big.bin"), JSON.stringify(found).slice(0, 200));
  const hello2 = await phone.req("hello");
  check("hello: storage and the Trash", (hello2.space || []).length > 0 && hello2.space[0].total > 0 && hello2.trash === H(".local/share/Trash/files"), JSON.stringify(hello2));
  const rest = await phone.req("restore", { path: H(".local/share/Trash/files/renamed") });
  check("restore from the Trash puts it back", !rest.err && existsSync(H("Downloads/renamed/a.txt")) && !existsSync(H(".local/share/Trash/info/renamed.trashinfo")), JSON.stringify(rest));
  await phone.ev(`window.__wado.files.showTransfers(true); true`);
  await sleep(500);
  await phone.shot("files-phone-transfers.png");
  await laptop.ev(`window.__wado.filesOpen(); true`);
  check("laptop: the window opens wide, on the overview, with a sidebar", await laptop.until(`!!document.querySelector('#wado-files[data-layout="wide"] .fside') && !!document.querySelector("#wado-files .fcats")`));
  await laptop.shot("files-laptop-home.png");
  await laptop.ev(`[...document.querySelectorAll("#wado-files .fside .fplace")].find((b) => b.textContent.includes("Home")).click(); true`);
  check("laptop: Home lists", await laptop.until(`document.querySelectorAll("#wado-files .frow").length > 3`));
  await laptop.ev(`(() => { const r = [...document.querySelectorAll("#wado-files .frow")].find((x) => x.textContent.includes("docs")); r.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true })); return true; })()`);
  check("laptop: right-click offers the actions", await laptop.until(`[...document.querySelectorAll(".wado-prompt .menurow")].some((b) => b.textContent === "Properties")`));
  await laptop.ev(`[...document.querySelectorAll(".wado-prompt .menurow")].find((b) => b.textContent === "Properties").click(); true`);
  check("laptop: Properties", await laptop.until(`(document.querySelector(".fmodal .fdialog")?.textContent || "").includes("Folder")`));
  await laptop.ev(`document.querySelector(".fmodal").remove(); true`);
  await laptop.ev(`(() => { const S = window.__wado.files.ui; S.searching = true; window.__wado.files.render(); const i = document.querySelector("#wado-files .fsearch input"); i.value = "doc"; i.dispatchEvent(new Event("input")); return true; })()`);
  check("laptop: the search box filters the folder as you type", await laptop.until(`[...document.querySelectorAll("#wado-files .frow .fname")].map((n) => n.textContent).every((t) => /doc/i.test(t)) && document.querySelectorAll("#wado-files .frow:not(.fhd)").length >= 1`));
  await laptop.ev(`(() => { const S = window.__wado.files.ui; S.searching = false; S.filter = ""; window.__wado.files.render(); return true; })()`);
  await laptop.ev(`window.__wado.files.ui.grid = true; window.__wado.files.render(); true`);
  await sleep(300);
  await laptop.shot("files-laptop-grid.png");
  await laptop.ev(`window.__wado.files.ui.grid = false; window.__wado.files.render(); true`);
  await laptop.ev(`(() => { const r = [...document.querySelectorAll("#wado-files .frow")].find((x) => x.textContent.includes("Pictures")); r.dispatchEvent(new MouseEvent("dblclick", { bubbles: true })); return true; })()`);
  await sleep(1200);
  await laptop.shot("files-laptop.png");
  // viewer
  await laptop.ev(`(() => { const r = [...document.querySelectorAll("#wado-files .frow")].find((x) => x.textContent.includes("p.png")); r.dispatchEvent(new MouseEvent("dblclick", { bubbles: true })); return true; })()`);
  check("viewer: an image opens", await laptop.until(`(document.querySelector(".fview .fvimg") || {}).naturalWidth === 64`));
  await laptop.shot("files-viewer.png");
  await laptop.ev(`document.querySelector(".fvnav.next").click(); true`);
  check("viewer: next goes through the folder's viewable files", await laptop.until(`document.querySelector(".fvtitle b")?.textContent === "page.html"`));
  check("viewer: HTML is shown as its source, never run", await laptop.until(`(document.querySelector(".fvtext")?.textContent || "").includes("<script>")`) && !(await laptop.ev(`!!window.__pwned`)));
  await laptop.ev(`document.querySelector(".fvnav.next").click(); true`);
  check("viewer: JSON is pretty-printed", await laptop.until(`(document.querySelector(".fvtext")?.textContent || "").includes('\n  "b": [')`));
  await laptop.ev(`document.querySelector(".fvnav.next").click(); true`);
  await laptop.ev(`(async () => { const F = window.__wado.files; const l = await F.req("list", { path: ${JSON.stringify(H("Pictures"))} });
    const all = l.entries.map((e) => ({ ...e, path: ${JSON.stringify(H("Pictures"))} + "/" + e.name })); F.view(all.find((e) => e.name === "tone.wav"), all); return true; })()`);
  check("viewer: audio plays", await laptop.until(`(document.querySelector(".fview audio") || {}).duration > 0.9`), await laptop.ev(`document.querySelector(".fvtitle b")?.textContent`));
  await laptop.ev(`document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })); true`);
  check("viewer: Esc closes it", await laptop.until(`!document.querySelector(".fview")`));
  // streaming
  const V = (n) => H("Videos/" + n);
  const pmkv = await laptop.req("probe", { path: V("other.mkv") });
  check("probe: tracks and embedded subtitles", (pmkv.audio || []).length === 2 && (pmkv.subs || []).some((x) => x.text) && Math.round(pmkv.duration) === 300, JSON.stringify(pmkv));
  const pmp4 = await laptop.req("probe", { path: V("film.mp4") });
  check("probe: a subtitle file beside it", (pmp4.sidecars || []).some((x) => x.name === "film.en.srt" && x.lang === "en"), JSON.stringify(pmp4.sidecars));
  const openMedia = (name) => laptop.ev(`(async () => { const F = window.__wado.files; const l = await F.req("list", { path: ${JSON.stringify(H("Videos"))} });
    const all = l.entries.map((e) => ({ ...e, path: ${JSON.stringify(H("Videos"))} + "/" + e.name }));
    F.view(all.find((e) => e.name === ${JSON.stringify(name)}), all); return true; })()`);
  const vstate = () => laptop.ev(`(() => { const v = document.querySelector(".fview video, .fview audio"); if (!v) return document.querySelector(".fvstage")?.innerText; const b = []; for (let i = 0; i < v.buffered.length; i++) b.push(v.buffered.start(i).toFixed(1) + "-" + v.buffered.end(i).toFixed(1));
    return JSON.stringify({ title: document.querySelector(".fvtitle b")?.textContent, rs: v.readyState, t: v.currentTime, paused: v.paused, err: v.error && v.error.message, buffered: b, src: v.src.slice(0, 30) }); })()`);
  const playing = (ms = 25000) => laptop.until(`(() => { const v = document.querySelector(".fview video, .fview audio"); if (!v) return false; window.__t0 ??= v.currentTime; return v.readyState >= 3 && !v.paused && v.currentTime > 0.5; })()`, ms);
  await openMedia("film.mp4");
  check("stream: an MP4 plays as it arrives", await playing(), JSON.stringify(await laptop.ev(`(() => { const v = document.querySelector(".fview video"); return v && { rs: v.readyState, t: v.currentTime, p: v.paused, err: v.error && v.error.message, stage: document.querySelector(".fvstage")?.innerText }; })()`)));
  check("stream: Plyr's controls are on it", await laptop.until(`!!document.querySelector(".fview .plyr .plyr__controls")`));
  check("captions: the file beside it turns on by itself", await laptop.until(`(document.querySelector(".fpsubs")?.textContent || "").includes("hello from the side file")`));
  await laptop.shot("files-player.png");
  await laptop.ev(`(() => { const v = document.querySelector(".fview video"); v.currentTime = 30; return true; })()`);
  check("stream: a seek past the buffer restarts it there", await laptop.until(`(() => { const v = document.querySelector(".fview video"); return v.currentTime > 30.2 && v.readyState >= 3; })()`, 20000),
    String(await laptop.ev(`document.querySelector(".fview video").currentTime`)));
  await laptop.ev(`document.dispatchEvent(new KeyboardEvent("keydown", { key: "]", bubbles: true })); true`);
  check("VLC keys: ] speeds up", await laptop.until(`document.querySelector(".fview video").playbackRate === 1.25`));
  await laptop.ev(`document.dispatchEvent(new KeyboardEvent("keydown", { key: "a", bubbles: true })); true`);
  check("VLC keys: a cycles the aspect", await laptop.until(`document.querySelector(".fview video").style.objectFit === "cover"`));
  const ffNice = () => { try { return execFileSync("sh", ["-c", "for p in $(pgrep -x ffmpeg); do grep -q wado-files-e2e /proc/$p/environ 2>/dev/null && ps -o ni= -p $p; done"]).toString().trim().split(/\s+/).filter(Boolean); } catch { return []; } };
  await laptop.ev(`document.dispatchEvent(new KeyboardEvent("keydown", { key: "n", bubbles: true })); true`);
  check("stream: an MKV the browser cannot play is converted and plays", await playing(30000), await vstate());
  const nice = ffNice();
  check("ffmpeg runs at the lowest priority (nice 19)", nice.length > 0 && nice.every((n) => n === "19"), JSON.stringify(nice));
  await laptop.ev(`document.querySelector(".fpvlcbtn").click(); true`);
  check("VLC menu: two audio tracks offered", await laptop.until(`[...document.querySelectorAll(".fpanel-vlc .fvopts button")].filter((b) => /English|French|eng|fra/.test(b.textContent)).length === 2`));
  await laptop.ev(`[...document.querySelectorAll(".fpanel-vlc .fvopts button")].find((b) => /French|fra/.test(b.textContent)).click(); true`);
  check("VLC menu: switching the audio track keeps playing", (await laptop.until(`(() => { const v = document.querySelector(".fview video"); return v.readyState >= 3 && !v.paused; })()`, 20000) || (console.log("        " + await vstate()), false))
    && await laptop.until(`[...document.querySelectorAll(".fpanel-vlc .fvopts button.on")].some((b) => /French|fra/.test(b.textContent))`));
  await laptop.ev(`[...document.querySelectorAll(".fpanel-vlc .fvopts button")].find((b) => b.textContent.includes("(in the file)")).click(); true`);
  check("captions: an embedded track", await laptop.until(`(document.querySelector(".fpsubs")?.textContent || "").includes("embedded words")`, 15000));
  await laptop.ev(`document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })); true`);
  await sleep(2500);
  check("closing the player stops the computer's ffmpeg", ffNice().length === 0, JSON.stringify(ffNice()));
  const vthumb = await laptop.req("thumb", { path: V("film.mp4") });
  check("a video gets a thumbnail (a frame)", !!vthumb.png && Buffer.from(vthumb.png, "base64").subarray(1, 4).toString() === "PNG", JSON.stringify(vthumb).slice(0, 160));
  // the player on a phone: slider on its own row above the buttons, gestures, landscape
  await phone.ev(`(async () => { const F = window.__wado.files; const l = await F.req("list", { path: ${JSON.stringify(H("Videos"))} });
    const all = l.entries.map((e) => ({ ...e, path: ${JSON.stringify(H("Videos"))} + "/" + e.name })); F.view(all.find((e) => e.name === "film.mp4"), all); return true; })()`);
  check("phone player: portrait is not mistaken for landscape", await phone.until(`!!document.querySelector(".fview .fplayer")`, 15000) && !(await phone.ev(`!!document.querySelector(".fview.land")`)));
  check("phone player: plays", await phone.until(`(() => { const v = document.querySelector(".fview video"); return v && v.readyState >= 3 && !v.paused; })()`, 25000));
  const geo = await phone.ev(`(() => { const q = (s) => document.querySelector(".fview " + s)?.getBoundingClientRect();
    const bar = q(".plyr__progress__container"), play = q(".plyr__controls [data-plyr='play']");
    return { barTop: bar && bar.top, playTop: play && play.top, barW: bar && bar.width, vw: innerWidth, vol: !!document.querySelector(".fview .plyr__volume input") }; })()`);
  check("phone player: the slider is a full-width row above the buttons, no volume slider",
    geo.barTop < geo.playTop - 10 && geo.barW > geo.vw * 0.85 && !geo.vol, JSON.stringify(geo));
  await sleep(500);
  await phone.ev(`(() => { const p = document.querySelector(".fview .plyr"); p.classList.remove("plyr--hide-controls"); return true; })()`);
  await sleep(700); // Plyr slides the controls in
  await phone.shot("files-phone-player.png");
  const tapAt = async (fx, n = 2) => {
    const r = await phone.ev(`(() => { const b = document.querySelector(".fview .plyr__video-wrapper").getBoundingClientRect(); return { x: b.left + b.width * ${fx}, y: b.top + b.height * 0.4 }; })()`);
    for (let i = 0; i < n; i++) {
      await phone.cdp("Input.dispatchTouchEvent", { type: "touchStart", touchPoints: [r] });
      await phone.cdp("Input.dispatchTouchEvent", { type: "touchEnd", touchPoints: [] });
      await sleep(80);
    }
  };
  const t0 = await phone.ev(`document.querySelector(".fview video").currentTime`);
  await tapAt(0.85);
  check("phone player: double-tap on the right skips 10 s", await phone.until(`document.querySelector(".fview video").currentTime > ${t0 + 8}`, 8000),
    `${t0} → ${await phone.ev(`document.querySelector(".fview video").currentTime`)}`);
  {
    const r = await phone.ev(`(() => { const b = document.querySelector(".fview .plyr__video-wrapper").getBoundingClientRect(); return { x: b.left + b.width / 2, y: b.top + b.height * 0.4 }; })()`);
    await phone.cdp("Input.dispatchTouchEvent", { type: "touchStart", touchPoints: [r] });
    await sleep(900);
    check("phone player: hold plays at 2×", await phone.ev(`document.querySelector(".fview video").playbackRate === 2`), String(await phone.ev(`document.querySelector(".fview video").playbackRate + " " + document.querySelector(".fpripple")?.className `)));
    await phone.cdp("Input.dispatchTouchEvent", { type: "touchEnd", touchPoints: [] });
    check("phone player: …and lets go back to 1×", await phone.until(`document.querySelector(".fview video").playbackRate === 1`, 3000));
  }
  await phone.cdp("Emulation.setDeviceMetricsOverride", { width: 915, height: 412, deviceScaleFactor: 2, mobile: true, screenOrientation: { type: "landscapePrimary", angle: 90 } });
  check("phone player: turned to landscape it goes edge to edge", await phone.until(`!!document.querySelector(".fview.land") && getComputedStyle(document.querySelector(".fview .fvhead")).display === "none"
    && document.querySelector(".fview .plyr__video-wrapper").getBoundingClientRect().height > innerHeight * 0.9`, 5000),
    await phone.ev(`JSON.stringify(document.querySelector(".fview .plyr__video-wrapper")?.getBoundingClientRect())`));
  await sleep(600);
  await phone.shot("files-phone-landscape.png");
  await phone.cdp("Emulation.setDeviceMetricsOverride", { width: 412, height: 915, deviceScaleFactor: 2, mobile: true, screenOrientation: { type: "portraitPrimary", angle: 0 } });
  check("phone player: …and back in portrait", await phone.until(`!document.querySelector(".fview.land")`, 5000));
  await phone.ev(`document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })); true`);
  // photos on a phone: a camera photo fits the screen; double-tap zooms (once, not in and out);
  // a single tap hides the bars; a swipe moves to the next photo
  await phone.ev(`(async () => { const F = window.__wado.files; const l = await F.req("list", { path: ${JSON.stringify(H("Pictures"))} });
    const all = l.entries.map((e) => ({ ...e, path: ${JSON.stringify(H("Pictures"))} + "/" + e.name })); F.view(all.find((e) => e.name === "zbig.jpg"), all); return true; })()`);
  check("phone photo: a 4000×3000 photo opens", await phone.until(`(document.querySelector(".fview .fvimg") || {}).naturalWidth === 4000`, 20000));
  await sleep(400);
  const fit = await phone.ev(`(() => { const s = document.querySelector(".fview .fvstage").getBoundingClientRect(), z = document.querySelector(".fview .fvzoom").getBoundingClientRect();
    return { stage: [s.width, s.height], layer: [z.width, z.height], vw: innerWidth }; })()`);
  check("phone photo: it fits the screen", Math.abs(fit.layer[0] - fit.stage[0]) < 2 && Math.abs(fit.layer[1] - fit.stage[1]) < 2 && fit.stage[0] <= fit.vw + 1, JSON.stringify(fit));
  await phone.shot("files-phone-photo.png");
  const centre = await phone.ev(`(() => { const b = document.querySelector(".fview .fvstage").getBoundingClientRect(); return { x: b.left + b.width / 2, y: b.top + b.height / 2 }; })()`);
  const touchTap = async () => { await phone.cdp("Input.dispatchTouchEvent", { type: "touchStart", touchPoints: [centre] }); await phone.cdp("Input.dispatchTouchEvent", { type: "touchEnd", touchPoints: [] }); };
  await touchTap(); await sleep(90); await touchTap();
  await sleep(900);
  check("phone photo: double-tap zooms in and stays zoomed", await phone.ev(`document.querySelector(".fview").classList.contains("zoomed")`),
    await phone.ev(`document.querySelector(".fview .fvzoom").style.transform`));
  await touchTap(); await sleep(90); await touchTap();
  check("phone photo: double-tap again fits it back", await phone.until(`!document.querySelector(".fview").classList.contains("zoomed")`, 3000));
  await sleep(400);
  await touchTap();
  check("phone photo: a single tap hides the bars", await phone.until(`document.querySelector(".fview").classList.contains("bare")`, 2000));
  await phone.shot("files-phone-photo-bare.png");
  const title0 = await phone.ev(`document.querySelector(".fvtitle b").textContent`);
  await phone.cdp("Input.dispatchTouchEvent", { type: "touchStart", touchPoints: [{ x: centre.x + 120, y: centre.y }] });
  for (const dx of [80, 20, -40, -100]) await phone.cdp("Input.dispatchTouchEvent", { type: "touchMove", touchPoints: [{ x: centre.x + dx, y: centre.y }] });
  await phone.cdp("Input.dispatchTouchEvent", { type: "touchEnd", touchPoints: [] });
  check("phone photo: a swipe moves to the next one", await phone.until(`document.querySelector(".fvtitle b").textContent !== ${JSON.stringify(title0)}`, 4000), title0);
  await phone.ev(`document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })); true`);
  // photo zoom
  await laptop.ev(`(async () => { const F = window.__wado.files; const l = await F.req("list", { path: ${JSON.stringify(H("Pictures"))} });
    const all = l.entries.map((e) => ({ ...e, path: ${JSON.stringify(H("Pictures"))} + "/" + e.name })); F.view(all.find((e) => e.name === "p.png"), all); return true; })()`);
  await laptop.until(`(document.querySelector(".fview .fvimg") || {}).naturalWidth === 64`);
  await laptop.ev(`document.querySelector(".fview .fvzoom").dispatchEvent(new MouseEvent("dblclick", { bubbles: true, clientX: 720, clientY: 450 })); true`);
  check("photos: double-click zooms in (Panzoom)", await laptop.until(`document.querySelector(".fview").classList.contains("zoomed") && /scale\((?!1\))/.test(document.querySelector(".fview .fvzoom").style.transform)`),
    await laptop.ev(`document.querySelector(".fview .fvzoom").style.transform + " panzoom=" + !!window.Panzoom`));
  await laptop.ev(`document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })); true`);
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
